use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use git_cmd::Repo;
use tracing::warn;

use crate::{
    diff::Commit,
    fs_utils,
    package_compare::{CARGO_VCS_INFO, is_generated_package_file},
};

use super::PackagePaths;

mod package_files;
mod replay;

pub(super) use package_files::HistoryPackageFiles;
use replay::{ChangeReplay, TokenConflicts};

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in an isolated object database to distinguish surviving contributions from
/// discarded merge parents. A revert that leaves the release unchanged proves a
/// change was absent there.
pub(super) struct RetainedChanges<'a> {
    repository: &'a Repo,
    head: String,
    /// The walked paths relative to the repository, as Git reports them.
    relative_paths: PackagePaths,
    /// The first equal snapshot found by the walk, standing in for the release.
    /// `None` until [`Self::add_boundary`] records one: nothing is pruned then.
    released: Option<String>,
    /// Repository-relative files Cargo packages at the release and, once
    /// [`Self::retain_surviving`] adds HEAD's, at HEAD. `None` when a listing
    /// failed: every file under the package directory counts then.
    package_files: Option<HashSet<Utf8PathBuf>>,
    /// Git's simplified, path-limited parent graph after release exclusions,
    /// with equal snapshot nodes removed to stop traversal at those boundaries.
    parents: HashMap<String, Vec<String>>,
    /// First commit of the simplified walk: HEAD only when HEAD touches the package.
    root: Option<String>,
    /// Commits reachable from `root` through the remaining parent graph.
    reachable: HashSet<String>,
    /// Full-history ancestors of every boundary: candidates for pruning.
    released_ancestors: HashSet<String>,
}

impl<'a> RetainedChanges<'a> {
    /// Prepare to prune the walk of `graph`, the simplified parent graph from
    /// `head` over the absolute `paths` that [`Repo::parents_at_paths`] reports,
    /// once [`Self::add_boundary`] finds an equal snapshot.
    pub(super) fn new(
        repository: &'a Repo,
        head: &str,
        graph: &[(String, Vec<String>)],
        paths: &PackagePaths,
    ) -> anyhow::Result<Self> {
        // The walked repository can be a temporary copy at a non-canonical path,
        // such as `/var` on macOS, while the README paths were canonicalized.
        // Canonicalization is best effort: the raw path is tried first anyway.
        let directory = repository.directory();
        let canonical_directory = fs_utils::canonicalize_utf8(directory).ok();
        let relativize = |path: &Utf8Path| {
            [Some(directory), canonical_directory.as_deref()]
                .into_iter()
                .flatten()
                .find_map(|base| path.strip_prefix(base).ok())
                .map(Utf8Path::to_path_buf)
                .with_context(|| format!("{path} is outside the repository {directory}"))
        };
        Ok(Self {
            repository,
            head: head.to_owned(),
            relative_paths: PackagePaths {
                package: relativize(&paths.package)?,
                readme: paths.readme.as_deref().map(relativize).transpose()?,
            },
            released: None,
            package_files: None,
            root: graph.first().map(|(commit, _)| commit.clone()),
            parents: graph.iter().cloned().collect(),
            reachable: HashSet::new(),
            released_ancestors: HashSet::new(),
        })
    }

    /// Join Cargo's package-relative `files` onto the repository-relative
    /// package directory, so they compare with the paths Git reports.
    fn repository_relative(
        &self,
        files: impl IntoIterator<Item = impl AsRef<Utf8Path>>,
    ) -> HashSet<Utf8PathBuf> {
        let package = &self.relative_paths.package;
        files.into_iter().map(|file| package.join(file)).collect()
    }

    /// Register an equal snapshot together with its full-history `ancestors`
    /// and the package-relative `package_files` Cargo lists for the release.
    ///
    /// Full ancestry also includes branches discarded by merges. An ancestor can
    /// nevertheless survive through another lineage; lineage reachability and the
    /// content check of [`Self::retain_surviving`] preserve them.
    pub(super) fn add_boundary(
        &mut self,
        commit: &str,
        ancestors: Vec<String>,
        package_files: &[Utf8PathBuf],
    ) {
        if self.released.is_none() {
            // Every equal snapshot packages the same comparable files, so the
            // first one stands in for the release.
            self.released = Some(commit.to_owned());
            self.package_files = Some(self.repository_relative(package_files));
        }
        // Remove only the boundary: its ancestors may still be reachable through
        // another lineage, which must remain available for the content check.
        self.parents.remove(commit);
        self.released_ancestors.extend(ancestors);
        self.reachable.clear();
        let mut pending: Vec<_> = self.root.iter().map(String::as_str).collect();
        while let Some(commit) = pending.pop() {
            let Some(parents) = self.parents.get(commit) else {
                continue;
            };
            if !self.reachable.insert(commit.to_owned()) {
                continue;
            }
            pending.extend(parents.iter().map(String::as_str));
        }
    }

    /// Whether the walk can skip `commit` without inspecting it: it is an
    /// ancestor of an equal snapshot and no other lineage reaches it without
    /// passing one. This is only an optimization; [`Self::retain_surviving`]
    /// makes the final decision with every discovered boundary.
    pub(super) fn skips(&self, commit: &str) -> bool {
        self.released_ancestors.contains(commit) && !self.reachable.contains(commit)
    }

    /// Whether `commit` is an ancestor of an equal snapshot that another lineage
    /// reaches without passing one: only the content check can decide about it.
    fn is_candidate(&self, commit: &str) -> bool {
        self.released_ancestors.contains(commit) && self.reachable.contains(commit)
    }

    /// Keep the `commits` of the walk that stay in the diff: those that are not
    /// ancestors of an equal snapshot, and those another lineage reaches without
    /// passing one whose change survives at HEAD. A simplified walk can visit an
    /// ancestor before the equal snapshot that prunes it, so this decision is
    /// load-bearing.
    ///
    /// `head_package_files` lists the package-relative files Cargo packages at
    /// HEAD, since a file added or removed since the release is only listed on
    /// one side; `None` makes every file under the package directory count.
    /// `head_package_files` only runs when `commits` contains a candidate.
    pub(super) fn retain_surviving(
        mut self,
        commits: &mut Vec<Commit>,
        head_package_files: impl FnOnce() -> anyhow::Result<Option<Vec<Utf8PathBuf>>>,
    ) -> anyhow::Result<()> {
        if !commits.iter().any(|commit| self.is_candidate(&commit.id)) {
            return Ok(());
        }
        self.add_package_files(head_package_files()?);
        // A replay that cannot be built, for example on a SHA-256 repository,
        // is reported once: without evidence, ancestry pruning then applies to
        // every candidate.
        let replay = ChangeReplay::new(self.repository, &self.head, &self.relative_paths.all())
            .inspect_err(|error| warn!("cannot check retained changes: {error:#}"))
            .ok();
        commits.retain(|commit| self.retains(replay.as_ref(), &commit.id));
        Ok(())
    }

    /// Add the package-relative files Cargo packages at another snapshot, or
    /// `None` when its listing failed.
    fn add_package_files(&mut self, files: Option<Vec<Utf8PathBuf>>) {
        let files = files.map(|files| self.repository_relative(files));
        match (self.package_files.as_mut(), files) {
            (Some(all), Some(files)) => all.extend(files),
            _ => self.package_files = None,
        }
    }

    /// Whether `commit` stays in the diff, see [`Self::retain_surviving`].
    fn retains(&self, replay: Option<&ChangeReplay<'_>>, commit: &str) -> bool {
        if !self.released_ancestors.contains(commit) {
            return true;
        }
        // A sibling editing the same lines as a reverted commit can make its
        // undo conflict. Trust that only for a candidate, which another lineage
        // reaches without passing an equal package snapshot.
        self.is_candidate(commit)
            && replay.is_some_and(|replay| {
                self.survives(replay, commit).unwrap_or_else(|error| {
                    // Shallow histories may not contain the parent required for a
                    // revert. Then there is no evidence to override ancestry pruning.
                    warn!("cannot check retained changes in {commit}: {error:#}");
                    false
                })
            })
    }

    /// Whether the change of `commit` was absent from the release and is still
    /// present at HEAD.
    fn survives(&self, replay: &ChangeReplay<'_>, commit: &str) -> anyhow::Result<bool> {
        // Candidates are ancestors of an equal snapshot, so one was recorded.
        let released = self
            .released
            .as_deref()
            .context("no equal snapshot was recorded")?;
        let includes = |path: &[u8]| self.includes(path);
        // The release contains the change, in part at least, when undoing it
        // changes the release. Conflicting tokens keep the release's: a change
        // can be absent even when its inverse conflicts with edits next to it.
        if replay.undo_affects_package(commit, released, TokenConflicts::FavorTarget, includes)? {
            return Ok(false);
        }
        // HEAD lacks the change when undoing it leaves HEAD as it is, whatever
        // else HEAD and the release disagree on. Conflicting tokens show HEAD's
        // own version of the change, which keeps its marker.
        if !replay.undo_affects_package(commit, &self.head, TokenConflicts::Unresolved, includes)? {
            return Ok(false);
        }
        // HEAD also lacks the change when it followed the release: the release's
        // edits since the change then leave HEAD as it is, even where they
        // evolved the change instead of undoing it.
        replay.edits_affect_package(
            commit,
            released,
            &self.head,
            TokenConflicts::Unresolved,
            includes,
        )
    }

    fn includes(&self, path: &[u8]) -> bool {
        let Ok(path) = std::str::from_utf8(path) else {
            // Cargo's UTF-8 file list cannot represent this path. Conservatively
            // include it only beneath the package directory; it cannot equal
            // the UTF-8 README path. Git uses '/' on every platform.
            let mut components = path.split(|byte| *byte == b'/');
            return self
                .relative_paths
                .package
                .iter()
                .all(|component| components.next() == Some(component.as_bytes()));
        };
        let path = Utf8Path::new(path);
        let PackagePaths { package, readme } = &self.relative_paths;
        // Package equality ignores the contents of every `Cargo.lock` and
        // `Cargo.toml.orig`. It also ignores `.cargo_vcs_info.json` at the package
        // root; nested copies are ordinary packaged files.
        let is_ignored = match path.file_name() {
            Some(CARGO_VCS_INFO) => path.parent() == Some(package.as_path()),
            name => name.is_some_and(is_generated_package_file),
        };
        if is_ignored {
            return false;
        }
        if readme.as_deref() == Some(path) {
            return true;
        }
        match &self.package_files {
            Some(files) => files.contains(path),
            None => path.starts_with(package),
        }
    }
}

/// A repository that reads the objects of `repository` through a read-only
/// alternate. Any `commit` id shows the object format: only SHA-1 repositories
/// are supported, since libgit2 cannot read SHA-256 objects.
fn read_only_objects(repository: &Repo, commit: &str) -> anyhow::Result<git2::Repository> {
    anyhow::ensure!(
        git2::Oid::from_str(commit).is_ok(),
        "SHA-256 repositories are not supported"
    );
    let objects = objects_directory(repository)?;
    let odb = git2::Odb::new()?;
    odb.add_disk_alternate(objects.as_str())?;
    Ok(git2::Repository::from_odb(odb)?)
}

/// The absolute path of the object database of `repository`.
///
/// Let Git resolve it: libgit2 cannot open repositories with some valid
/// extensions, such as `extensions.partialClone`. Git reports the path relative
/// to the repository directory unless it is absolute, as in linked worktrees.
fn objects_directory(repository: &Repo) -> anyhow::Result<Utf8PathBuf> {
    let objects = repository.git(&["rev-parse", "--git-path", "objects"])?;
    fs_utils::canonicalize_utf8(&repository.directory().join(objects))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_utf8_paths_are_scoped_to_the_package_directory() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        let repo = Repo::init(dir.path());
        let package = Utf8Path::new("crates").join("pkg");
        let paths = PackagePaths {
            package: repo.directory().join(&package),
            readme: Some(repo.directory().join("README.md")),
        };
        let head = repo.current_commit_hash().unwrap();
        let mut changes = RetainedChanges::new(&repo, &head, &[], &paths).unwrap();
        changes.package_files = Some(HashSet::from([package.join("src/lib.rs")]));

        for (path, included) in [
            (b"crates/pkg/src/lib.rs".as_slice(), true),
            (b"crates/pkg/ignored.txt", false),
            (b"README.md", true),
            (b"crates/pkg/src/\xff", true),
            (b"crates/pkg-extra/\xff", false),
            (b"outside/\xff", false),
            (b"crates/\xff/pkg/file", false),
        ] {
            assert_eq!(changes.includes(path), included, "{path:?}");
        }
        changes.package_files = None;
        assert!(changes.includes(b"crates/pkg/ignored.txt"));
        assert!(changes.includes(b"crates/pkg/\xff"));
        assert!(!changes.includes(b"crates/pkg-extra/\xff"));

        // A package at the repository root conservatively includes every path
        // whose bytes cannot be checked against its Cargo file list.
        changes.relative_paths.package = Utf8PathBuf::new();
        changes.package_files = Some(HashSet::new());
        assert!(changes.includes(b"\xff"));
        assert!(changes.includes(b"src/\xff"));
    }

    /// Commit `contents` to `file` at the repository root and return the commit hash.
    fn commit_file(repo: &Repo, contents: &str) -> String {
        fs_err::write(repo.directory().join("file"), contents).unwrap();
        repo.add_all_and_commit("change file").unwrap();
        repo.current_commit_hash().unwrap()
    }

    /// The retained changes of the walk from `changed` with the single equal
    /// snapshot `released`, treating the repository root as the package
    /// directory, and the replay of the change. `package_files` are its
    /// packaged files, or every file when `None`.
    fn retained_changes<'a>(
        repo: &'a Repo,
        changed: &str,
        released: &str,
        package_files: Option<Vec<Utf8PathBuf>>,
    ) -> (RetainedChanges<'a>, ChangeReplay<'a>) {
        let paths = PackagePaths {
            package: repo.directory().to_path_buf(),
            readme: None,
        };
        let graph = repo
            .parents_at_paths(changed, &[], &paths.all(), None)
            .unwrap();
        let mut changes = RetainedChanges::new(repo, changed, &graph, &paths).unwrap();
        changes.add_boundary(
            released,
            vec![released.to_owned()],
            package_files.as_deref().unwrap_or_default(),
        );
        changes.add_package_files(package_files);
        let replay = ChangeReplay::new(repo, changed, &changes.relative_paths.all()).unwrap();
        (changes, replay)
    }

    #[test]
    fn global_merge_attributes_are_isolated() {
        let config = fs_utils::Utf8TempDir::new().unwrap();
        fs_err::create_dir(config.path().join("git")).unwrap();
        fs_err::write(config.path().join("git/attributes"), "* merge=union\n").unwrap();
        // The test harness matches on the path without the crate name.
        let child = format!(
            "{}::merge_configuration_is_isolated_without_changing_worktree_indexes",
            module_path!().split_once("::").unwrap().1
        );
        // Run the existing fixture in a fresh process: changing this process's
        // environment is not safe while other tests are running.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &child])
            .env("XDG_CONFIG_HOME", config.path())
            .env("RELEASE_PLZ_TEST_GLOBAL_MERGE_ATTRIBUTES", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "child test failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // An obsolete --exact filter must not silently skip the regression.
        assert!(stdout.contains("1 passed; 0 failed"), "{stdout}");
    }

    #[test]
    fn merge_configuration_is_isolated_without_changing_worktree_indexes() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        fs_err::create_dir(dir.path().join("main")).unwrap();
        let repo = Repo::init(dir.path().join("main"));
        commit_file(&repo, "a\n");
        let changed = commit_file(&repo, "b\n");
        let released = commit_file(&repo, "c\n");
        if std::env::var_os("RELEASE_PLZ_TEST_GLOBAL_MERGE_ATTRIBUTES").is_some() {
            // Confirm the child really loads the global rule in ordinary Git.
            assert_eq!(
                repo.git(&["check-attr", "merge", "--", "file"]).unwrap(),
                "file: merge: union"
            );
        }
        fs_err::write(repo.directory().join(".gitattributes"), "* merge=union\n").unwrap();
        repo.add_all_and_commit("merge attributes").unwrap();
        repo.git(&["config", "merge.default", "union"]).unwrap();
        let attributes_path = repo.directory().join(".git/info/attributes");
        fs_err::write(&attributes_path, "* merge=union\n").unwrap();
        let config_path = repo.directory().join(".git/config");
        let config_before = fs_err::read(&config_path).unwrap();
        let linked = dir.path().join("linked");
        repo.git(&["worktree", "add", "--detach", linked.as_str()])
            .unwrap();

        for path in [repo.directory(), &linked] {
            let source = Repo::new(path).unwrap();
            let index_path = path.join(source.git(&["rev-parse", "--git-path", "index"]).unwrap());
            let index_before = fs_err::read(&index_path).unwrap();
            let head_before = source.current_commit_hash().unwrap();
            let objects_before = source.git(&["count-objects", "-v"]).unwrap();
            let contents_before = fs_err::read(path.join("file")).unwrap();
            let (changes, replay) = retained_changes(&source, &changed, &released, None);

            assert!(changes.survives(&replay, &changed).unwrap());
            assert_eq!(fs_err::read(&index_path).unwrap(), index_before);
            assert_eq!(source.current_commit_hash().unwrap(), head_before);
            assert_eq!(
                source.git(&["count-objects", "-v"]).unwrap(),
                objects_before
            );
            assert_eq!(fs_err::read(path.join("file")).unwrap(), contents_before);
        }
        assert_eq!(fs_err::read(&config_path).unwrap(), config_before);
        assert_eq!(
            fs_err::read_to_string(attributes_path).unwrap(),
            "* merge=union\n"
        );
    }

    #[test]
    fn release_conflicts_do_not_prove_partly_released_or_binary_changes_absent() {
        for (base, changed, released) in [
            (
                "a\n1\n2\n3\n4\n5\n6\n7\n8\n9\nx\n",
                "b\n1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n",
                "c\n1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n",
            ),
            ("a\0", "b\0", "c\0"),
        ] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = Repo::init(dir.path());
            commit_file(&repo, base);
            let changed = commit_file(&repo, changed);
            let released = commit_file(&repo, released);
            let (changes, replay) = retained_changes(&repo, &changed, &released, None);
            // A nonconflicting hunk still undoes a released change. Binary
            // conflicts cannot establish absence by choosing the release's bytes.
            assert!(!changes.survives(&replay, &changed).unwrap());
        }
    }

    #[test]
    fn a_root_commit_can_be_replayed() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        let repo = Repo::init(dir.path());
        // Repo::init creates a root commit containing README.md.
        let root = repo.current_commit_hash().unwrap();
        repo.git(&["rm", "README.md"]).unwrap();
        repo.add_all_and_commit("remove root's file").unwrap();
        let released = repo.current_commit_hash().unwrap();
        let (changes, replay) = retained_changes(&repo, &root, &released, None);
        assert!(changes.survives(&replay, &root).unwrap());
    }

    #[test]
    fn a_merge_commit_is_replayed_relative_to_its_first_parent() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        let repo = Repo::init(dir.path());
        fs_err::write(repo.directory().join("file"), "a\n").unwrap();
        repo.add_all_and_commit("base").unwrap();
        repo.git(&["checkout", "-b", "feature"]).unwrap();
        fs_err::write(repo.directory().join("file"), "b\n").unwrap();
        repo.add_all_and_commit("change file").unwrap();
        repo.checkout_head().unwrap();
        fs_err::write(repo.directory().join("unrelated"), "mainline\n").unwrap();
        repo.add_all_and_commit("mainline").unwrap();
        repo.git(&["merge", "--no-ff", "-m", "merge feature", "feature"])
            .unwrap();
        let changed = repo.current_commit_hash().unwrap();
        fs_err::write(repo.directory().join("file"), "a\n").unwrap();
        repo.add_all_and_commit("revert merged change").unwrap();
        let released = repo.current_commit_hash().unwrap();
        let (changes, replay) = retained_changes(
            &repo,
            &changed,
            &released,
            Some(vec![Utf8PathBuf::from("file")]),
        );
        // The first parent has the old file. Reverting relative to the
        // second parent would only remove the unrelated mainline file.
        assert!(changes.survives(&replay, &changed).unwrap());
    }

    #[test]
    fn undoing_a_deletion_re_adds_the_original_packaged_path() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        let repo = Repo::init(dir.path());
        fs_err::create_dir(repo.directory().join("old")).unwrap();
        fs_err::write(repo.directory().join("old/a"), "unchanged\n").unwrap();
        fs_err::write(repo.directory().join("old/b"), "removed\n").unwrap();
        repo.add_all_and_commit("add files").unwrap();
        repo.git(&["rm", "old/b"]).unwrap();
        repo.add_all_and_commit("remove packaged file").unwrap();
        let changed = repo.current_commit_hash().unwrap();
        repo.git(&["mv", "old", "new"]).unwrap();
        repo.add_all_and_commit("rename directory").unwrap();
        let target = repo.current_commit_hash().unwrap();
        let (changes, replay) = retained_changes(
            &repo,
            &target,
            &target,
            Some(vec![Utf8PathBuf::from("old/b")]),
        );
        // libgit2 does not follow the directory rename: undoing the deletion
        // re-adds old/b without a conflict, and that original path is packaged.
        assert!(
            replay
                .undo_affects_package(&changed, &target, TokenConflicts::Unresolved, |path| {
                    changes.includes(path)
                })
                .unwrap()
        );
    }
}
