use std::path::Path;

use super::*;

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in memory to distinguish surviving contributions from discarded merge parents.
/// A revert that leaves the release unchanged proves a change was absent there.
pub(super) struct RetainedChanges {
    /// The walked repository, opened in memory: no snapshot is ever checked out.
    repo: git2::Repository,
    /// The branch tip whose surviving changes are being released.
    head: git2::Oid,
    /// The first equal snapshot found by the walk, standing in for the release.
    released: git2::Oid,
    /// Repository-relative files Cargo packages at HEAD and at the release, or
    /// `None` when listing failed and every file under `paths` counts.
    package_files: Option<HashSet<Utf8PathBuf>>,
    /// Repository-relative paths: the package directory first, then the canonical
    /// target of the configured README, if any.
    paths: Vec<Utf8PathBuf>,
    /// Git's simplified, path-limited parent graph after release exclusions.
    parents: HashMap<String, Vec<String>>,
    /// First commit of the simplified walk: HEAD only when HEAD touches the package.
    root: Option<String>,
    /// Equal snapshots found so far; every lineage stops there.
    boundaries: HashSet<String>,
    /// Commits reachable from `root` without passing a boundary.
    reachable: HashSet<String>,
    /// Full-history ancestors of every boundary: candidates for pruning.
    released_ancestors: HashSet<String>,
}

impl RetainedChanges {
    pub(super) fn new(
        repository: &Repo,
        head: &str,
        released: &str,
        release_boundaries: &[&str],
        package_files: Option<HashSet<Utf8PathBuf>>,
        paths: &[Utf8PathBuf],
    ) -> anyhow::Result<Self> {
        // Match the outer walk's release exclusions, including ignoring commits
        // missing from a shallow clone or rewritten history.
        let exclusions: Vec<_> = release_boundaries
            .iter()
            .filter_map(|boundary| {
                repository
                    .git(&[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("{boundary}^{{commit}}"),
                    ])
                    .ok()
            })
            .map(|commit| format!("^{commit}"))
            .collect();
        let mut args = vec!["rev-list", "--parents", "--date-order", head];
        args.extend(exclusions.iter().map(String::as_str));
        args.push("--");
        args.extend(paths.iter().map(|path| path.as_str()));
        let graph = repository.git(&args)?;
        let root = graph.split_whitespace().next().map(str::to_owned);
        let parents = graph
            .lines()
            .filter_map(|line| {
                let mut ids = line.split_whitespace();
                Some((ids.next()?.to_owned(), ids.map(str::to_owned).collect()))
            })
            .collect();
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
            repo: git2::Repository::open(directory)?,
            head: git2::Oid::from_str(head)?,
            released: git2::Oid::from_str(released)?,
            package_files,
            paths: paths
                .iter()
                .map(|path| relativize(path))
                .collect::<anyhow::Result<_>>()?,
            parents,
            root,
            boundaries: HashSet::new(),
            reachable: HashSet::new(),
            released_ancestors: HashSet::new(),
        })
    }

    /// Register an equal snapshot together with its full-history `ancestors`.
    ///
    /// Full ancestry also includes branches discarded by merges. An ancestor can
    /// nevertheless survive through another lineage; lineage reachability and the
    /// content check of [`Self::retains`] preserve them.
    pub(super) fn add_boundary(&mut self, commit: &str, ancestors: Vec<String>) {
        self.boundaries.insert(commit.to_owned());
        self.released_ancestors.extend(ancestors);
        self.reachable.clear();
        let mut pending: Vec<_> = self.root.iter().map(String::as_str).collect();
        while let Some(commit) = pending.pop() {
            if self.boundaries.contains(commit) || !self.reachable.insert(commit.to_owned()) {
                continue;
            }
            if let Some(parents) = self.parents.get(commit) {
                pending.extend(parents.iter().map(String::as_str));
            }
        }
    }

    /// Whether the walk can skip `commit` without inspecting it: it is an
    /// ancestor of an equal snapshot and no other lineage reaches it without
    /// passing one. This is only an optimization; [`Self::retains`] makes the
    /// final decision with every discovered boundary.
    pub(super) fn skips(&self, commit: &str) -> bool {
        self.released_ancestors.contains(commit) && !self.reaches(commit)
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
        self.reaches(commit)
            && self.survives(commit).unwrap_or_else(|error| {
                // Shallow histories may not contain the parent required for a
                // revert. Then there is no evidence to override ancestry pruning.
                warn!("cannot check retained changes in {commit}: {error:#}");
                false
            })
    }

    fn reaches(&self, commit: &str) -> bool {
        self.reachable.contains(commit)
    }

    /// Whether the change of `commit` was absent from the release and is still
    /// present at HEAD.
    fn survives(&self, commit: &str) -> anyhow::Result<bool> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit)?)?;
        let released = self.repo.find_commit(self.released)?;
        // A conflict against the release does not establish that the commit's
        // contribution was absent from it. Preserve the existing pruning then.
        if self.undo_changes_package(&commit, &released)? {
            return Ok(false);
        }
        let head = self.repo.find_commit(self.head)?;
        // Once absence from the release is established, a conflict at HEAD is
        // ambiguous: keep the commit rather than losing a breaking-change marker.
        self.undo_changes_package(&commit, &head)
    }

    /// Whether undoing `commit` on `target` changes the packaged files. A conflict
    /// counts as a change: it does not prove that the contribution was absent.
    fn undo_changes_package(
        &self,
        commit: &git2::Commit<'_>,
        target: &git2::Commit<'_>,
    ) -> anyhow::Result<bool> {
        // For merge commits, undo the change relative to the first parent, as
        // `git revert -m 1` does. Root commits are handled by libgit2's empty base.
        let mainline = u32::from(commit.parent_count() > 1);
        let index = self.repo.revert_commit(commit, target, mainline, None)?;
        let tree = target.tree()?;
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            let affects_package = [
                conflict.ancestor.as_ref(),
                conflict.our.as_ref(),
                conflict.their.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|entry| {
                std::str::from_utf8(&entry.path)
                    .map(|path| self.includes(Path::new(path)))
                    .unwrap_or(true)
            });
            if affects_package {
                return Ok(true);
            }
        }
        let diff = self
            .repo
            .diff_tree_to_index(Some(&tree), Some(&index), None)?;
        Ok(diff.deltas().any(|delta| {
            [delta.old_file().path(), delta.new_file().path()]
                .into_iter()
                .flatten()
                .any(|path| self.includes(path))
        }))
    }

    fn includes(&self, path: &Path) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        if matches!(
            path.file_name(),
            Some("Cargo.lock" | CARGO_TOML_ORIG | CARGO_VCS_INFO)
        ) {
            return false;
        }
        if let Some(files) = &self.package_files {
            files.contains(path) || self.paths.iter().skip(1).any(|readme| path == readme)
        } else {
            self.paths.iter().any(|root| path.starts_with(root))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_graph_excludes_release_boundaries() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        let repo = Repo::init(dir.path());
        let commit_file = |name: &str| {
            fs_err::write(repo.directory().join(name), name).unwrap();
            repo.add_all_and_commit(name).unwrap();
            repo.current_commit_hash().unwrap()
        };
        let baseline = commit_file("baseline.rs");
        repo.git(&["branch", "published"]).unwrap();
        let tagged = commit_file("tagged.rs");
        repo.tag("v0.1.0", "release").unwrap();
        let mainline = commit_file("mainline.rs");
        repo.git(&["checkout", "published"]).unwrap();
        let published = commit_file("published.rs");
        let sibling = commit_file("sibling.rs");
        repo.checkout_head().unwrap();
        repo.git(&["merge", "--no-ff", "-m", "merge published", "published"])
            .unwrap();
        let head = repo.current_commit_hash().unwrap();
        let graph_commits = |boundaries: &[&str]| {
            RetainedChanges::new(
                &repo,
                &head,
                &tagged,
                boundaries,
                None,
                &[repo.directory().to_path_buf()],
            )
            .unwrap()
            .parents
            .into_keys()
            .collect::<HashSet<_>>()
        };
        let missing = "0".repeat(40);
        let unbounded = graph_commits(&[]);
        assert!(unbounded.contains(&baseline));
        assert_eq!(graph_commits(&[&missing]), unbounded);
        // The tag and published SHA can bound different branches. Keep both
        // unreleased lineages while excluding both releases and shared ancestry.
        assert_eq!(
            graph_commits(&["v0.1.0", &published, &missing]),
            HashSet::from([head, mainline, sibling])
        );
    }
}
