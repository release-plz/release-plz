use std::{fmt::Write as _, path::Path};

use cargo_metadata::camino::Utf8Component;

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

    pub(super) fn contains(&self, commit: &str) -> bool {
        if !self.reaches(commit) {
            return false;
        }
        let contribution = self.check(commit).unwrap_or_else(|error| {
            // Shallow histories may not contain the parent required for a revert.
            // In that case there is no evidence to override ancestry pruning.
            debug!("cannot check retained changes in {commit}: {error:#}");
            Contribution::Absent
        });
        match contribution {
            Contribution::Absent => false,
            // A sibling editing the same lines as a reverted commit can make its
            // undo conflict. The reachability check above ensures another lineage
            // reaches it without passing an equal package snapshot.
            Contribution::Present | Contribution::Conflict => true,
        }
    }

    pub(super) fn reaches(&self, commit: &str) -> bool {
        self.reachable.contains(commit)
    }

    fn check(&self, commit: &str) -> anyhow::Result<Contribution> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit)?)?;
        let released = self.repo.find_commit(self.released)?;
        // A conflict against the release does not establish that the commit's
        // contribution was absent from it. Preserve the existing pruning then.
        if !matches!(
            self.undo_changes_package(&commit, &released)?,
            Contribution::Absent
        ) {
            return Ok(Contribution::Absent);
        }
        let head = self.repo.find_commit(self.head)?;
        // Once absence from the release is established, a conflict at HEAD is
        // ambiguous: keep the commit rather than losing a breaking-change marker.
        self.undo_changes_package(&commit, &head)
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
        let readme_changed = self.readme_changed(&tree, &index);
        let readme_unchanged = readme_changed == Some(false);
        let mut changed = readme_changed == Some(true);
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
                    .map(|path| {
                        self.includes(
                            Path::new(path),
                            changes_file_presence,
                            &tree,
                            readme_unchanged,
                        )
                    })
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
                .any(|path| self.includes(path, changes_file_presence, &tree, readme_unchanged))
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

    /// README equality follows symlinks. Different link spellings can therefore
    /// undo to exactly the same contents, including in a pointer-only conflict.
    fn readme_changed(&self, target: &git2::Tree<'_>, reverted: &git2::Index) -> Option<bool> {
        let readme = self.readme.as_deref()?;
        let target = self.readme_blob(readme, |path| {
            let entry = target.get_path(path.as_std_path()).ok()?;
            Some((entry.id(), entry.filemode().try_into().ok()?))
        })?;
        let mut conflicted = false;
        let reverted = self.readme_blob(readme, |path| {
            let entry = match reverted.get_path(path.as_std_path(), 0) {
                Some(entry) => entry,
                // All three stages at this path establish a content conflict,
                // rather than an addition, deletion or rename. The inverse's
                // candidate can still read the same bytes as the current alias.
                None if path == readme
                    && reverted.get_path(path.as_std_path(), 1).is_some()
                    && reverted.get_path(path.as_std_path(), 2).is_some() =>
                {
                    conflicted = true;
                    reverted.get_path(path.as_std_path(), 3)?
                }
                None => {
                    if (1..=3).any(|stage| reverted.get_path(path.as_std_path(), stage).is_some()) {
                        return None;
                    }
                    // Indices store files rather than directories. A resolved
                    // descendant proves this prefix is a real directory.
                    let prefix =
                        format!("{}/", path.as_str().replace(std::path::MAIN_SEPARATOR, "/"));
                    let child = reverted.get(reverted.find_prefix(prefix.as_str()).ok()?)?;
                    let child_path = Path::new(std::str::from_utf8(&child.path).ok()?);
                    reverted.get_path(child_path, 0)?;
                    return Some((git2::Oid::ZERO_SHA1, 0o040_000));
                }
            };
            Some((entry.id, entry.mode))
        })?;
        if target == reverted {
            Some(false)
        } else if conflicted {
            // A differing conflict candidate is not the resolved inverse. Keep
            // the ordinary text-conflict refinement instead of claiming a change.
            None
        } else {
            Some(true)
        }
    }

    /// Resolve only repository-relative links in Git objects, never on disk.
    /// Missing, cyclic, absolute or escaping links leave the check conservative.
    fn readme_blob(
        &self,
        readme: &Utf8Path,
        mut lookup: impl FnMut(&Utf8Path) -> Option<(git2::Oid, u32)>,
    ) -> Option<git2::Oid> {
        let mut path = readme.to_path_buf();
        for _ in 0..40 {
            if path.as_str().is_empty() {
                return None;
            }
            let (id, mode) = lookup(&path)?;
            match mode {
                mode if self.is_regular_file_in_checkout(mode) => return Some(id),
                0o120_000 => {
                    let blob = self.repo.find_blob(id).ok()?;
                    let target = std::str::from_utf8(blob.content()).ok()?;
                    if target.is_empty() || target.contains('\0') {
                        return None;
                    }
                    let mut resolved = path.parent()?.to_path_buf();
                    for component in Utf8Path::new(target).components() {
                        match component {
                            Utf8Component::Normal(name) => resolved.push(name),
                            Utf8Component::CurDir => {}
                            // `link/..` follows the directory link before moving
                            // up, so lexical normalization would be incorrect.
                            Utf8Component::ParentDir
                                if !resolved.as_str().is_empty()
                                    && lookup(&resolved)?.1 == 0o040_000
                                    && resolved.pop() => {}
                            _ => return None,
                        }
                    }
                    path = resolved;
                }
                _ => return None,
            }
        }
        None
    }

    fn is_regular_file_in_checkout(&self, mode: u32) -> bool {
        matches!(mode, 0o100_644 | 0o100_755) || (!self.symlinks && mode == 0o120_000)
    }

    fn includes(
        &self,
        path: &Path,
        changes_file_presence: bool,
        target: &git2::Tree<'_>,
        readme_unchanged: bool,
    ) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        if self.readme.as_deref() == Some(path) {
            return !readme_unchanged;
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
