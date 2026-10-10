//! Reuse a package's file list across the checkouts of a history walk.
//!
//! The walk checks commits out in a clean temporary copy of the repository, so
//! the tracked files on disk are those of the checked-out commit, and untracked
//! and ignored files stay the same across checkouts. Cargo selects packaged
//! files by their paths, types and modes, manifests, ignore rules and
//! configuration, never by the contents of Rust sources. Commits whose trees
//! differ only in the contents of non-executable `.rs` files therefore package
//! the same files. Configuration outside the repository is assumed stable
//! during a walk, just as for the released-package cache. Whenever the trees
//! cannot be compared, Cargo lists the files again.

use std::{cell::OnceCell, path::Path};

use git_cmd::Repo;
use tracing::debug;

use crate::package_compare::PackageFiles;

use super::replay::read_only_objects;

/// The package files of a checkout, reused by the following checkouts while
/// their trees differ from its tree only in the contents of Rust sources.
pub(crate) struct HistoryPackageFiles<'a> {
    repository: &'a Repo,
    /// Reads the trees of the checked-out commits, opened at the first
    /// checkout. `None` when the objects cannot be read: then Cargo lists the
    /// files of every checkout.
    repo: OnceCell<Option<git2::Repository>>,
    /// The tree of the last checkout, which `files` belongs to. `None` before
    /// the first checkout and when that tree cannot be read.
    tree: Option<git2::Oid>,
    files: PackageFiles,
}

impl<'a> HistoryPackageFiles<'a> {
    /// Prepare to walk the history of `repository`.
    pub(crate) fn new(repository: &'a Repo) -> Self {
        Self {
            repository,
            repo: OnceCell::new(),
            tree: None,
            files: PackageFiles::default(),
        }
    }

    /// The package files at `commit`. Call this after checking `commit` out,
    /// before inspecting the package.
    pub(crate) fn files_at(&mut self, commit: &str) -> &PackageFiles {
        match self.reuses_files_at(commit) {
            Ok(true) => {
                if self.files.is_cached() {
                    debug!("reusing historical package file list at {commit}");
                }
            }
            Ok(false) => self.files = PackageFiles::default(),
            Err(error) => {
                debug!(
                    "cannot compare the package files at {commit} with the cached list: {error:#}"
                );
                self.files = PackageFiles::default();
            }
        }
        &self.files
    }

    /// Whether the files listed at the previous checkout are those at `commit`,
    /// whose tree becomes the previous one.
    fn reuses_files_at(&mut self, commit: &str) -> anyhow::Result<bool> {
        // Forget the previous tree unless `commit`'s tree replaces it.
        let previous = self.tree.take();
        // Opening the objects runs Git, so skip it for packages without history.
        let repo = self.repo.get_or_init(|| {
            read_only_objects(self.repository, commit)
                .inspect_err(|error| {
                    debug!("cannot reuse historical package file lists: {error:#}");
                })
                .ok()
        });
        let Some(repo) = repo else {
            return Ok(false);
        };
        let tree = repo.find_commit(git2::Oid::from_str(commit)?)?.tree()?;
        self.tree = Some(tree.id());
        // Differing only in Rust sources is transitive, so comparing with the
        // previous checkout covers every checkout since the list was computed.
        let Some(previous) = previous else {
            return Ok(false);
        };
        only_rust_sources_differ(repo, &repo.find_tree(previous)?, &tree)
    }
}

/// Whether the trees `old` and `new` differ only in the contents of Rust
/// sources that are non-executable regular files in both. Additions, deletions
/// and renames, which a diff without rename detection reports as both, change
/// the file list. Symlinks and submodules stand for other files, and executable
/// scripts can be Cargo or rustc wrappers.
fn only_rust_sources_differ(
    repo: &git2::Repository,
    old: &git2::Tree<'_>,
    new: &git2::Tree<'_>,
) -> anyhow::Result<bool> {
    // Without rename detection, the diff reads no blobs.
    let diff = repo.diff_tree_to_tree(Some(old), Some(new), None)?;
    for delta in diff.deltas() {
        if delta.status() != git2::Delta::Modified {
            return Ok(false);
        }
        // `DiffFile::path` panics on non-UTF-8 paths on Windows, which Cargo
        // cannot list anyway.
        let path = delta
            .new_file()
            .path_bytes()
            .and_then(|path| std::str::from_utf8(path).ok());
        let Some(path) = path.filter(|path| path.ends_with(".rs")) else {
            return Ok(false);
        };
        // Read the modes from the trees, since `DiffFile::mode` panics on
        // legacy modes Git accepts, such as 100600.
        for tree in [old, new] {
            if tree.get_path(Path::new(path))?.filemode() != i32::from(git2::FileMode::Blob) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
