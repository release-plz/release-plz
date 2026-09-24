use std::{fmt::Write as _, path::Path};

use super::*;

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in memory to distinguish surviving contributions from discarded merge parents.
/// A revert that leaves the release unchanged proves a change was absent there.
pub(super) struct RetainedChanges {
    /// The walked repository, opened without a worktree: no snapshot is ever
    /// checked out and no snapshot's `.gitattributes` applies.
    repo: git2::Repository,
    /// The branch tip whose surviving changes are being released.
    head: git2::Oid,
    /// The first equal snapshot found by the walk, standing in for the release.
    released: git2::Oid,
    /// Repository-relative files Cargo packages at the release and, once
    /// [`Self::add_package_files`] ran, at HEAD. `None` when a listing failed:
    /// every file under `paths` counts then.
    package_files: Option<HashSet<Utf8PathBuf>>,
    /// Repository-relative paths: the package directory first, then the canonical
    /// target of the configured README, if any.
    paths: Vec<Utf8PathBuf>,
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

impl RetainedChanges {
    /// Start from the first equal snapshot `released` and its full-history
    /// `ancestors`, as in [`Self::add_boundary`].
    pub(super) fn new(
        repository: &Repo,
        head: &str,
        released: &str,
        ancestors: Vec<String>,
        release_boundaries: &[&str],
        package_files: Option<HashSet<Utf8PathBuf>>,
        paths: &[Utf8PathBuf],
    ) -> anyhow::Result<Self> {
        // Follow the outer walk's simplification and release exclusions.
        let graph = repository.parents_at_paths(head, release_boundaries, paths)?;
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
        // libgit2 resolves merge drivers such as `merge=union` from the
        // `.gitattributes` of the worktree or index, which would make the content
        // check depend on the snapshot checked out when it runs. Only trees are
        // needed: reopen the git directory without a worktree and with an empty
        // index, so text conflicts always use the default driver.
        let git_dir = git2::Repository::open(directory)?.path().to_path_buf();
        let repo = git2::Repository::open_bare(git_dir)?;
        repo.set_index(&mut git2::Index::new()?)?;
        let mut changes = Self {
            repo,
            head: git2::Oid::from_str(head)?,
            released: git2::Oid::from_str(released)?,
            package_files,
            paths: paths
                .iter()
                .map(|path| relativize(path))
                .collect::<anyhow::Result<_>>()?,
            root: graph.first().map(|(commit, _)| commit.clone()),
            parents: graph.into_iter().collect(),
            reachable: HashSet::new(),
            released_ancestors: HashSet::new(),
        };
        changes.add_boundary(released, ancestors);
        Ok(changes)
    }

    /// Register an equal snapshot together with its full-history `ancestors`.
    ///
    /// Full ancestry also includes branches discarded by merges. An ancestor can
    /// nevertheless survive through another lineage; lineage reachability and the
    /// content check of [`Self::retains`] preserve them.
    pub(super) fn add_boundary(&mut self, commit: &str, ancestors: Vec<String>) {
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

    /// Add the files Cargo packages at another snapshot, typically HEAD: a file
    /// added or removed since the release is only listed on one side. A failed
    /// listing (`None`) makes every file under `paths` count.
    pub(super) fn add_package_files(&mut self, files: Option<HashSet<Utf8PathBuf>>) {
        self.package_files = self
            .package_files
            .take()
            .zip(files)
            .map(|(mut all, files)| {
                all.extend(files);
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
            && self.survives(commit).unwrap_or_else(|error| {
                // Shallow histories may not contain the parent required for a
                // revert. Then there is no evidence to override ancestry pruning.
                warn!("cannot check retained changes in {commit}: {error:#}");
                false
            })
    }

    /// Whether the change of `commit` was absent from the release and is still
    /// present at HEAD.
    fn survives(&self, commit: &str) -> anyhow::Result<bool> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit)?)?;
        let released = self.repo.find_commit(self.released)?;
        // Resolve supported text conflicts in favor of the release: an
        // overwritten change can be absent even when its inverse conflicts.
        if self.undo_changes_package(&commit, &released, git2::FileFavor::Ours)? {
            return Ok(false);
        }
        let head = self.repo.find_commit(self.head)?;
        // At HEAD, a clean character-level merge can establish that the change
        // was already undone despite later edits on the same line. Keep actual
        // character conflicts: an evolved change may still require its marker.
        self.undo_changes_package(&commit, &head, git2::FileFavor::Normal)
    }

    /// Whether undoing `commit` on `target` changes the packaged files. Unresolved
    /// conflicts count as changes. Supported text conflicts are retried at
    /// character granularity, favoring the target only for the release check.
    fn undo_changes_package(
        &self,
        commit: &git2::Commit<'_>,
        target: &git2::Commit<'_>,
        text_conflict_favor: git2::FileFavor,
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
            if affects_package
                && !self.conflict_leaves_file_unchanged(&conflict, text_conflict_favor)?
            {
                return Ok(true);
            }
        }
        let diff = self
            .repo
            .diff_tree_to_index(Some(&tree), Some(&index), None)?;
        Ok(diff
            .deltas()
            // Conflicted paths were checked above, including any nonconflicting
            // hunks in the same file. Only clean index changes remain to check.
            .filter(|delta| delta.status() != git2::Delta::Conflicted)
            .any(|delta| {
                [delta.old_file().path(), delta.new_file().path()]
                    .into_iter()
                    .flatten()
                    .any(|path| self.includes(path))
            }))
    }

    /// Whether a text conflict's inverse leaves the target unchanged. Independent
    /// edits on the same line still apply; conflicting characters keep the
    /// target's content only when checking the release.
    /// Binary, large, rename, deletion, and mode conflicts cannot establish absence.
    fn conflict_leaves_file_unchanged(
        &self,
        conflict: &git2::IndexConflict,
        favor: git2::FileFavor,
    ) -> anyhow::Result<bool> {
        let (Some(ancestor), Some(ours), Some(theirs)) =
            (&conflict.ancestor, &conflict.our, &conflict.their)
        else {
            return Ok(false);
        };
        if ancestor.path != ours.path
            || theirs.path != ours.path
            || ancestor.mode != ours.mode
            || theirs.mode != ours.mode
            || !matches!(ours.mode, 0o100_644 | 0o100_755)
        {
            return Ok(false);
        }
        let blobs = [
            self.repo.find_blob(ancestor.id)?,
            self.repo.find_blob(ours.id)?,
            self.repo.find_blob(theirs.id)?,
        ];
        let max_conflict_input_bytes = 1024 * 1024; // 1 MiB across all three snapshots.
        // One character per line lets libgit2 distinguish independent edits on
        // the same source line. Bound the expanded input and leave binary or
        // non-UTF-8 content unresolved.
        if blobs.iter().map(git2::Blob::size).sum::<usize>() > max_conflict_input_bytes
            || blobs.iter().any(|blob| blob.content().contains(&0))
        {
            return Ok(false);
        }
        let mut encoded = Vec::with_capacity(blobs.len());
        for blob in &blobs {
            let Ok(contents) = std::str::from_utf8(blob.content()) else {
                return Ok(false);
            };
            let mut characters = String::with_capacity(contents.len() * 3);
            for character in contents.chars() {
                writeln!(characters, "{:x}", u32::from(character))?;
            }
            encoded.push(characters);
        }
        let [mut ancestor, mut ours, mut theirs] =
            std::array::from_fn(|_| git2::MergeFileInput::new());
        ancestor.content(encoded[0].as_bytes());
        ours.content(encoded[1].as_bytes());
        theirs.content(encoded[2].as_bytes());
        let mut options = git2::MergeFileOptions::new();
        options.favor(favor);
        let merged = git2::merge_file(&ancestor, &ours, &theirs, Some(&mut options))?;
        Ok(merged.is_automergeable() && merged.content() == encoded[1].as_bytes())
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
            let commit_file = |contents: &str| {
                fs_err::write(repo.directory().join("file"), contents).unwrap();
                repo.add_all_and_commit("change file").unwrap();
                repo.current_commit_hash().unwrap()
            };
            commit_file(base);
            let changed = commit_file(changed);
            let released = commit_file(released);
            let changes = RetainedChanges::new(
                &repo,
                &changed,
                &released,
                vec![released.clone()],
                &[],
                None,
                &[repo.directory().to_path_buf()],
            )
            .unwrap();
            // A nonconflicting hunk still undoes a released change. Binary
            // conflicts cannot establish absence by choosing the release's bytes.
            assert!(!changes.survives(&changed).unwrap());
        }
    }
}
