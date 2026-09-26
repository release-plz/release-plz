use std::{cell::OnceCell, path::Path};

mod replay;

use replay::ChangeReplay;

use super::*;

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
    /// The replay backend, built on first use so that an unusable one only
    /// disables the content check, see [`Self::replay`].
    replay: OnceCell<Option<ChangeReplay>>,
    head: String,
    release_boundaries: Vec<String>,
    /// Absolute paths, as the outer walk limits Git to.
    paths: PackagePaths,
    /// The same paths relative to the repository, as Git reports them.
    relative_paths: PackagePaths,
    /// The first equal snapshot found by the walk, standing in for the release.
    /// `None` until [`Self::add_boundary`] records one: nothing is pruned then.
    released: Option<String>,
    /// Repository-relative files Cargo packages at the release and, once
    /// [`Self::add_package_files`] ran, at HEAD. `None` when a listing failed:
    /// every file under `paths` counts then.
    package_files: Option<HashSet<Utf8PathBuf>>,
    /// Git's simplified, path-limited parent graph after release exclusions,
    /// with equal snapshot nodes removed to stop traversal at those boundaries.
    /// Built with the first boundary, since only pruning needs it.
    parents: HashMap<String, Vec<String>>,
    /// First commit of the simplified walk: HEAD only when HEAD touches the package.
    root: Option<String>,
    /// Commits reachable from `root` through the remaining parent graph.
    reachable: HashSet<String>,
    /// Full-history ancestors of every boundary: candidates for pruning.
    released_ancestors: HashSet<String>,
}

impl<'a> RetainedChanges<'a> {
    /// Prepare to prune the walk from `head` over the absolute `paths`, excluding
    /// `release_boundaries`, once [`Self::add_boundary`] finds an equal snapshot.
    pub(super) fn new(
        repository: &'a Repo,
        head: &str,
        release_boundaries: &[&str],
        paths: &PackagePaths,
    ) -> anyhow::Result<Self> {
        // The walked repository can be a temporary copy at a non-canonical path,
        // such as `/var` on macOS, while the README paths were canonicalized.
        // Canonicalization is best effort: the raw path is tried first anyway.
        let directory = repository.directory();
        let canonical_directory = crate::fs_utils::canonicalize_utf8(directory).ok();
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
            replay: OnceCell::new(),
            head: head.to_owned(),
            release_boundaries: release_boundaries
                .iter()
                .copied()
                .map(str::to_owned)
                .collect(),
            paths: paths.clone(),
            relative_paths: PackagePaths {
                package: relativize(&paths.package)?,
                readme: paths.readme.as_deref().map(relativize).transpose()?,
            },
            released: None,
            package_files: None,
            parents: HashMap::new(),
            root: None,
            reachable: HashSet::new(),
            released_ancestors: HashSet::new(),
        })
    }

    /// Join Cargo's package-relative `files` onto the repository-relative
    /// package directory, so they compare with the paths Git reports.
    fn repository_relative(
        &self,
        files: impl IntoIterator<Item = Utf8PathBuf>,
    ) -> HashSet<Utf8PathBuf> {
        let package = &self.relative_paths.package;
        files.into_iter().map(|file| package.join(file)).collect()
    }

    /// Register an equal snapshot together with its full-history `ancestors`
    /// and the package-relative `package_files` Cargo lists there.
    ///
    /// Full ancestry also includes branches discarded by merges. An ancestor can
    /// nevertheless survive through another lineage; lineage reachability and the
    /// content check of [`Self::retains`] preserve them.
    pub(super) fn add_boundary(
        &mut self,
        commit: &str,
        ancestors: Vec<String>,
        package_files: Vec<Utf8PathBuf>,
    ) -> anyhow::Result<()> {
        if self.released.is_none() {
            // Follow the outer walk's simplification and release exclusions.
            let exclude: Vec<&str> = self.release_boundaries.iter().map(String::as_str).collect();
            let graph =
                self.repository
                    .parents_at_paths(&self.head, &exclude, &self.paths.all())?;
            self.root = graph.first().map(|(commit, _)| commit.clone());
            self.parents = graph.into_iter().collect();
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
        Ok(())
    }

    /// Whether [`Self::add_boundary`] recorded an equal snapshot, so that
    /// [`Self::retains`] can prune.
    pub(super) fn has_boundary(&self) -> bool {
        self.released.is_some()
    }

    /// Add the package-relative files Cargo packages at another snapshot,
    /// typically HEAD: a file added or removed since the release is only listed
    /// on one side. A failed listing (`None`) makes every file under `paths` count.
    pub(super) fn add_package_files(&mut self, files: Option<Vec<Utf8PathBuf>>) {
        self.package_files = self
            .package_files
            .take()
            .zip(files)
            .map(|(mut all, files)| {
                all.extend(self.repository_relative(files));
                all
            });
    }

    /// Whether the walk can skip `commit` without inspecting it: it is an
    /// ancestor of an equal snapshot and no other lineage reaches it without
    /// passing one. This is only an optimization; [`Self::retains`] makes the
    /// final decision with every discovered boundary.
    pub(super) fn skips(&self, commit: &str) -> bool {
        self.released_ancestors.contains(commit) && !self.reachable.contains(commit)
    }

    /// Whether `commit` stays in the diff: either it is not an ancestor of an
    /// equal snapshot, or another lineage reaches it without passing one and its
    /// change survives at HEAD. A simplified walk can visit an ancestor before
    /// the equal snapshot that prunes it, so this decision is load-bearing.
    pub(super) fn retains(&self, commit: &str) -> bool {
        if !self.released_ancestors.contains(commit) {
            return true;
        }
        // A sibling editing the same lines as a reverted commit can make its
        // undo conflict. Reachability ensures another lineage reaches the commit
        // without passing an equal package snapshot before trusting that.
        self.reachable.contains(commit)
            && self.replay().is_some_and(|replay| {
                self.survives(replay, commit).unwrap_or_else(|error| {
                    // Shallow histories may not contain the parent required for a
                    // revert. Then there is no evidence to override ancestry pruning.
                    warn!("cannot check retained changes in {commit}: {error:#}");
                    false
                })
            })
    }

    /// The replay backend, built on first use. One that cannot be built, for
    /// example with Git older than 2.40 on a SHA-256 repository, is reported
    /// once: without evidence, ancestry pruning then applies to every candidate.
    fn replay(&self) -> Option<&ChangeReplay> {
        self.replay
            .get_or_init(|| {
                ChangeReplay::new(self.repository)
                    .inspect_err(|error| warn!("cannot check retained changes: {error:#}"))
                    .ok()
            })
            .as_ref()
    }

    /// Whether the change of `commit` was absent from the release and is still
    /// present at HEAD.
    fn survives(&self, replay: &ChangeReplay, commit: &str) -> anyhow::Result<bool> {
        // Candidates are ancestors of an equal snapshot, so one was recorded.
        let released = self
            .released
            .as_deref()
            .context("no equal snapshot was recorded")?;
        let change = replay.change(commit)?;
        // Resolve supported text conflicts in favor of the release: an
        // overwritten change can be absent even when its inverse conflicts.
        // Known limitation: when the release edited tokens adjacent to the
        // change, the token-level merge conflicts, favoring the release leaves
        // its content unchanged, and the change counts as absent. A commit also
        // reachable through another lineage can then be re-reported. Accepted:
        // it only reproduces the pre-existing behavior for that commit.
        if change.undo_changes_package(released, true, |path| self.includes(path))? {
            return Ok(false);
        }
        // At HEAD, a clean token-level merge can establish that the change
        // was already undone despite later edits on the same line. Keep actual
        // token conflicts: an evolved change may still require its marker.
        change.undo_changes_package(&self.head, false, |path| self.includes(path))
    }

    fn includes(&self, path: &Path) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        if path
            .file_name()
            .is_some_and(crate::package_compare::is_generated_package_file)
        {
            return false;
        }
        let PackagePaths { package, readme } = &self.relative_paths;
        if let Some(files) = &self.package_files {
            files.contains(path) || readme.as_deref() == Some(path)
        } else {
            path.starts_with(package)
                || readme
                    .as_deref()
                    .is_some_and(|readme| path.starts_with(readme))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(path: &Utf8Path, object_format: &str) -> Repo {
        git_cmd::git_in_dir(path, &["init", &format!("--object-format={object_format}")]).unwrap();
        Repo::init(path)
    }

    /// Commit `contents` to `file` at the repository root and return the commit hash.
    fn commit_file(repo: &Repo, contents: &str) -> String {
        fs_err::write(repo.directory().join("file"), contents).unwrap();
        repo.add_all_and_commit("change file").unwrap();
        repo.current_commit_hash().unwrap()
    }

    /// Replay `changed` against the single equal snapshot `released`, treating
    /// the repository root as the package directory. `package_files` are its
    /// packaged files, or every file when `None`.
    fn replay<'a>(
        repo: &'a Repo,
        changed: &str,
        released: &str,
        package_files: Option<Vec<Utf8PathBuf>>,
    ) -> RetainedChanges<'a> {
        let paths = PackagePaths {
            package: repo.directory().to_path_buf(),
            readme: None,
        };
        let mut changes = RetainedChanges::new(repo, changed, &[], &paths).unwrap();
        changes
            .add_boundary(
                released,
                vec![released.to_owned()],
                package_files.clone().unwrap_or_default(),
            )
            .unwrap();
        changes.add_package_files(package_files);
        changes
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
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            fs_err::create_dir(dir.path().join("main")).unwrap();
            let repo = init_repo(&dir.path().join("main"), object_format);
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
                let index_path = source
                    .git(&["rev-parse", "--path-format=absolute", "--git-path", "index"])
                    .unwrap();
                let index_before = fs_err::read(&index_path).unwrap();
                let head_before = source.current_commit_hash().unwrap();
                let objects_before = source.git(&["count-objects", "-v"]).unwrap();
                let contents_before = fs_err::read(path.join("file")).unwrap();
                let changes = replay(&source, &changed, &released, None);

                assert!(
                    changes
                        .survives(changes.replay().unwrap(), &changed)
                        .unwrap()
                );
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
    }

    #[test]
    fn release_conflicts_do_not_prove_partly_released_or_binary_changes_absent() {
        for object_format in ["sha1", "sha256"] {
            for (base, changed, released) in [
                (
                    "a\n1\n2\n3\n4\n5\n6\n7\n8\n9\nx\n",
                    "b\n1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n",
                    "c\n1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n",
                ),
                ("a\0", "b\0", "c\0"),
            ] {
                let dir = fs_utils::Utf8TempDir::new().unwrap();
                let repo = init_repo(dir.path(), object_format);
                commit_file(&repo, base);
                let changed = commit_file(&repo, changed);
                let released = commit_file(&repo, released);
                let changes = replay(&repo, &changed, &released, None);
                // A nonconflicting hunk still undoes a released change. Binary
                // conflicts cannot establish absence by choosing the release's bytes.
                assert!(
                    !changes
                        .survives(changes.replay().unwrap(), &changed)
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn a_root_commit_can_be_replayed() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = init_repo(dir.path(), object_format);
            // Repo::init creates a root commit containing README.md.
            let root = repo.current_commit_hash().unwrap();
            repo.git(&["rm", "README.md"]).unwrap();
            repo.add_all_and_commit("remove root's file").unwrap();
            let released = repo.current_commit_hash().unwrap();
            let changes = replay(&repo, &root, &released, None);
            assert!(changes.survives(changes.replay().unwrap(), &root).unwrap());
        }
    }

    #[test]
    fn a_merge_commit_is_replayed_relative_to_its_first_parent() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = init_repo(dir.path(), object_format);
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
            let changes = replay(
                &repo,
                &changed,
                &released,
                Some(vec![Utf8PathBuf::from("file")]),
            );
            // The first parent has the old file. Reverting relative to the
            // second parent would only remove the unrelated mainline file.
            assert!(
                changes
                    .survives(changes.replay().unwrap(), &changed)
                    .unwrap()
            );
        }
    }

    #[test]
    fn directory_rename_conflicts_include_the_original_packaged_path() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = init_repo(dir.path(), object_format);
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
            let changes = replay(
                &repo,
                &target,
                &target,
                Some(vec![Utf8PathBuf::from("old/b")]),
            );
            // Undoing the deletion suggests new/b, but only the original old/b
            // is packaged. Git's structural conflict still affects this package.
            assert!(
                changes
                    .replay()
                    .unwrap()
                    .change(&changed)
                    .unwrap()
                    .undo_changes_package(&target, false, |path| changes.includes(path))
                    .unwrap()
            );
        }
    }
}
