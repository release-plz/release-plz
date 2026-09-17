use std::{fmt::Write as _, path::Path};

use super::*;

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in memory to distinguish surviving contributions from discarded merge parents.
/// A revert that leaves the release unchanged proves a change was absent there.
/// Compare conflicting text at character granularity so independent edits to the
/// same line do not obscure that proof.
pub(super) struct RetainedChanges {
    repo: git2::Repository,
    symlinks: bool,
    head: git2::Oid,
    released: git2::Oid,
    package_files: Option<HashSet<Utf8PathBuf>>,
    paths: Vec<Utf8PathBuf>,
    readme: Option<Utf8PathBuf>,
    parents: HashMap<String, Vec<String>>,
    root: Option<String>,
    boundaries: HashSet<String>,
    reachable: HashSet<String>,
}

enum Contribution {
    Absent,
    Present,
    Conflict,
}

impl RetainedChanges {
    pub(super) fn new(
        repository: &Repo,
        head: &str,
        released: &str,
        package_files: Option<HashSet<Utf8PathBuf>>,
        paths: &[Utf8PathBuf],
        readme: Option<&Utf8Path>,
    ) -> anyhow::Result<Self> {
        let mut args = vec!["rev-list", "--parents", "--date-order", head, "--"];
        args.extend(paths.iter().map(|path| path.as_str()));
        let graph = repository.git(&args)?;
        // Match the native Git process that checks out historical snapshots,
        // including configuration supplied through environment overrides.
        let symlinks = repository
            .git(&["config", "--type=bool", "--get", "core.symlinks"])
            .map_or(true, |value| value == "true");
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
        let directory = repository.directory();
        let canonical_directory = crate::fs_utils::canonicalize_utf8(directory)?;
        let relativize = |path: &Utf8Path| {
            path.strip_prefix(directory)
                .or_else(|_| path.strip_prefix(&canonical_directory))
                .map(Utf8Path::to_path_buf)
                .with_context(|| format!("{path} is outside the repository {directory}"))
        };
        Ok(Self {
            repo: git2::Repository::open(directory)?,
            symlinks,
            head: git2::Oid::from_str(head)?,
            released: git2::Oid::from_str(released)?,
            package_files,
            paths: paths
                .iter()
                .map(|path| relativize(path))
                .collect::<anyhow::Result<_>>()?,
            readme: readme.map(relativize).transpose()?,
            parents,
            root,
            boundaries: HashSet::new(),
            reachable: HashSet::new(),
        })
    }

    pub(super) fn add_boundary(&mut self, commit: &str) {
        self.boundaries.insert(commit.to_owned());
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

    /// Whether `commit` stays in the diff: another lineage reaches it without
    /// passing an equal snapshot, and its change survives at HEAD.
    pub(super) fn retains(&self, commit: &str) -> bool {
        // A sibling editing the same lines as a reverted commit can make its
        // undo conflict. Reachability ensures another lineage reaches the commit
        // without passing an equal package snapshot before trusting that.
        self.reaches(commit)
            && self.survives(commit).unwrap_or_else(|error| {
                // Shallow histories may not contain the parent required for a
                // revert. Then there is no evidence to override ancestry pruning.
                debug!("cannot check retained changes in {commit}: {error:#}");
                false
            })
    }

    pub(super) fn reaches(&self, commit: &str) -> bool {
        self.reachable.contains(commit)
    }

    /// Whether the change of `commit` was absent from the release and is still
    /// present at HEAD.
    fn survives(&self, commit: &str) -> anyhow::Result<bool> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit)?)?;
        let released = self.repo.find_commit(self.released)?;
        // A conflict against the release does not establish that the commit's
        // contribution was absent from it. Preserve the existing pruning then.
        let absent_from_release = matches!(
            self.undo_changes_package(&commit, &released)?,
            Contribution::Absent
        );
        if !absent_from_release {
            return Ok(false);
        }
        let head = self.repo.find_commit(self.head)?;
        // Once absence from the release is established, a conflict at HEAD is
        // ambiguous: keep the commit rather than losing a breaking-change marker.
        Ok(!matches!(
            self.undo_changes_package(&commit, &head)?,
            Contribution::Absent
        ))
    }

    fn undo_changes_package(
        &self,
        commit: &git2::Commit<'_>,
        target: &git2::Commit<'_>,
    ) -> anyhow::Result<Contribution> {
        // For merge commits, undo the change relative to the first parent, as
        // `git revert -m 1` does. Root commits are handled by libgit2's empty base.
        let mainline = u32::from(commit.parent_count() > 1);
        let index = self.repo.revert_commit(commit, target, mainline, None)?;
        let tree = target.tree()?;
        let mut changed = false;
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            let changes_file_presence = conflict.our.as_ref().map(|entry| &entry.path)
                != conflict.their.as_ref().map(|entry| &entry.path);
            let affects_package = [
                conflict.ancestor.as_ref(),
                conflict.our.as_ref(),
                conflict.their.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|entry| {
                std::str::from_utf8(&entry.path)
                    .map(|path| self.includes(Path::new(path), changes_file_presence, &tree))
                    .unwrap_or(true)
            });
            if affects_package {
                let refined = self
                    .refine_text_conflict(&conflict)
                    .unwrap_or_else(|error| {
                        debug!("cannot refine retained text changes: {error:#}");
                        Contribution::Conflict
                    });
                match refined {
                    Contribution::Absent => {}
                    Contribution::Present => changed = true,
                    Contribution::Conflict => return Ok(Contribution::Conflict),
                }
            }
        }
        // Replacing a regular file with a symlink keeps the same packaged path.
        let mut options = git2::DiffOptions::new();
        options.include_typechange(true);
        let diff = self
            .repo
            .diff_tree_to_index(Some(&tree), Some(&index), Some(&mut options))?;
        changed |= diff.deltas().any(|delta| {
            // Conflicts were classified above without changing the in-memory
            // index; do not count a refined no-op again as a changed file.
            if delta.status() == git2::Delta::Conflicted {
                return false;
            }
            // Package equality compares file contents, not executable bits.
            if delta.old_file().id() == delta.new_file().id()
                && [delta.old_file().mode(), delta.new_file().mode()]
                    .into_iter()
                    .all(|mode| self.is_regular_file_in_checkout(u32::from(mode)))
            {
                return false;
            }
            let changes_file_presence =
                matches!(delta.status(), git2::Delta::Added | git2::Delta::Deleted);
            [delta.old_file().path(), delta.new_file().path()]
                .into_iter()
                .flatten()
                .any(|path| self.includes(path, changes_file_presence, &tree))
        });
        Ok(if changed {
            Contribution::Present
        } else {
            Contribution::Absent
        })
    }

    fn refine_text_conflict(&self, conflict: &git2::IndexConflict) -> anyhow::Result<Contribution> {
        let (Some(ancestor), Some(ours), Some(theirs)) =
            (&conflict.ancestor, &conflict.our, &conflict.their)
        else {
            return Ok(Contribution::Conflict);
        };
        if ancestor.path != ours.path
            || ancestor.path != theirs.path
            || ![ancestor.mode, ours.mode, theirs.mode]
                .into_iter()
                .all(|mode| self.is_regular_file_in_checkout(mode))
        {
            return Ok(Contribution::Conflict);
        }
        let blobs = [
            self.repo.find_blob(ancestor.id)?,
            self.repo.find_blob(ours.id)?,
            self.repo.find_blob(theirs.id)?,
        ];
        // Character-per-line inputs expand the diff's working set. Keep large
        // or binary conflicts unresolved instead of allocating unbounded tokens.
        if blobs.iter().map(git2::Blob::size).sum::<usize>() > 1024 * 1024
            || blobs.iter().any(|blob| blob.content().contains(&0))
        {
            return Ok(Contribution::Conflict);
        }
        let mut encoded = Vec::with_capacity(blobs.len());
        for blob in &blobs {
            let Ok(contents) = std::str::from_utf8(blob.content()) else {
                return Ok(Contribution::Conflict);
            };
            let mut characters = String::with_capacity(contents.len() * 3);
            for character in contents.chars() {
                // Encoding preserves newlines and Unicode scalars as individual,
                // unambiguous lines for libgit2's existing three-way text merge.
                writeln!(characters, "{:x}", u32::from(character))?;
            }
            encoded.push(characters);
        }
        let [mut ancestor, mut ours, mut theirs] =
            std::array::from_fn(|_| git2::MergeFileInput::new());
        ancestor.content(encoded[0].as_bytes());
        ours.content(encoded[1].as_bytes());
        theirs.content(encoded[2].as_bytes());
        let merged = git2::merge_file(&ancestor, &ours, &theirs, None)?;
        Ok(if !merged.is_automergeable() {
            Contribution::Conflict
        } else if merged.content() == encoded[1].as_bytes() {
            Contribution::Absent
        } else {
            Contribution::Present
        })
    }

    fn is_regular_file_in_checkout(&self, mode: u32) -> bool {
        matches!(mode, 0o100_644 | 0o100_755) || (!self.symlinks && mode == 0o120_000)
    }

    fn includes(&self, path: &Path, changes_file_presence: bool, target: &git2::Tree<'_>) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        // Package equality compares the configured README separately, following
        // links. Any change to it counts, including retargeting the link itself.
        if self.readme.as_deref() == Some(path) {
            return true;
        }
        // Match package equality: generated files are ignored at the package root.
        let package_relative_path = self
            .paths
            .first()
            .and_then(|root| path.strip_prefix(root).ok());
        if matches!(
            package_relative_path.map(Utf8Path::as_str),
            Some("Cargo.lock" | CARGO_TOML_ORIG | CARGO_VCS_INFO)
        ) {
            return false;
        }
        // Nested lockfiles and original manifests contribute to the package file
        // list even though their contents are excluded from equality checks.
        if !changes_file_presence
            && matches!(path.file_name(), Some("Cargo.lock" | CARGO_TOML_ORIG))
        {
            return false;
        }
        // Equality ignores symlink contents in the local snapshot, but their
        // addition or deletion still changes the package's file list. The
        // manifest and configured README are compared separately, following links.
        if !changes_file_presence
            && self.symlinks
            && package_relative_path.map(Utf8Path::as_str) != Some(CARGO_TOML)
            && target
                .get_path(path.as_std_path())
                .is_ok_and(|entry| entry.filemode() == i32::from(git2::FileMode::Link))
        {
            return false;
        }
        if let Some(files) = &self.package_files {
            files.contains(path)
                // An overridden README can live outside the package directory.
                || self.paths.iter().skip(1).any(|readme| path == readme)
        } else {
            // File listing failures already make package commit filtering
            // conservative; retain that behavior for the content check as well.
            self.paths.iter().any(|root| path.starts_with(root))
        }
    }
}
