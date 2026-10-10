//! Reuse a package's file list across the checkouts of a history walk.
//!
//! The walk checks commits out in a clean temporary copy of the repository, so
//! the tracked files on disk are those of the checked-out commit, and
//! untracked and ignored files change only at checkouts that add or remove
//! tracked paths. Cargo selects packaged files by their paths and types,
//! manifests, ignore rules and configuration, never by the contents of Rust
//! sources. Commits whose trees differ only in the contents of non-executable
//! `.rs` files therefore package the same files, unless a symlinked manifest
//! makes Cargo read a `.rs` file as TOML. Configuration outside the repository
//! is assumed stable during a walk, just as for the released-package cache.
//! Whenever the checked-out trees cannot be compared or contain a symlinked
//! manifest, Cargo lists the files again.

use anyhow::Context as _;
use git_cmd::Repo;
use tracing::debug;

use crate::package_compare::PackageFiles;

/// The package files of a checkout, reused by the following checkouts while
/// their trees differ from its tree only in the contents of Rust sources.
pub(crate) struct CheckoutPackageFiles<'a> {
    repository: &'a Repo,
    /// The last checked-out commit, whose files `files` lists. `None` before the
    /// first checkout and when the tree of that commit contains, or cannot be
    /// checked for, a symlinked manifest.
    commit: Option<String>,
    files: PackageFiles,
}

impl<'a> CheckoutPackageFiles<'a> {
    /// Prepare to walk the history of `repository`.
    pub(crate) fn new(repository: &'a Repo) -> Self {
        Self {
            repository,
            commit: None,
            files: PackageFiles::default(),
        }
    }

    /// The package files at `commit`. Call this after checking `commit` out,
    /// before inspecting the package.
    pub(crate) fn files_at(&mut self, commit: &str) -> &PackageFiles {
        // Differing only in Rust sources is transitive, so comparing with the
        // previous checkout covers every checkout since the list was computed.
        let reuses = self.commit.take().is_some_and(|previous| {
            only_rust_sources_differ(self.repository, &previous, commit).unwrap_or_else(|error| {
                debug!("cannot compare the trees of {previous} and {commit}: {error:#}");
                false
            })
        });
        if !reuses {
            self.files = PackageFiles::default();
        } else if self.files.is_cached() {
            debug!("reusing historical package file list at {commit}");
        }
        // A source-only diff also preserves the absence of symlinked manifests.
        // Scan the full tree only when starting a new cached list.
        let cacheable = reuses
            || has_symlinked_manifest(self.repository, commit)
                .inspect_err(|error| {
                    debug!("cannot find symlinked manifests at {commit}: {error:#}");
                })
                .is_ok_and(|found| !found);
        if cacheable {
            self.commit = Some(commit.to_owned());
        }
        &self.files
    }
}

/// A manifest can point to a regular `.rs` file whose contents affect Cargo's
/// file selection. Inspect nested packages too, including workspace members.
fn has_symlinked_manifest(repository: &Repo, commit: &str) -> anyhow::Result<bool> {
    let tree = repository.git(&["ls-tree", "-r", "-z", commit])?;
    // Each entry is `<mode> <type> <id>\t<path>`, NUL-terminated.
    Ok(tree.split_terminator('\0').any(|entry| {
        entry.split_once('\t').is_some_and(|(info, path)| {
            // Cargo.toml can resolve to another casing on case-insensitive filesystems.
            let name = path.rsplit('/').next().unwrap_or(path);
            info.starts_with("120000 ") && name.eq_ignore_ascii_case("Cargo.toml")
        })
    }))
}

/// Whether the trees of the commits `old` and `new` differ only in the contents
/// of Rust sources that are non-executable regular files in both. Additions,
/// deletions and renames, which a diff without rename detection reports as
/// both, change the file list. Symlinks and submodules stand for other files,
/// and executable scripts can be Cargo or rustc wrappers.
fn only_rust_sources_differ(repository: &Repo, old: &str, new: &str) -> anyhow::Result<bool> {
    // `diff-tree` detects no renames, so it reads no blobs. Report submodule
    // updates even if `submodule.<name>.ignore` hides them.
    let diff = repository.git(&[
        "diff-tree",
        "-r",
        "-z",
        "--ignore-submodules=none",
        old,
        new,
    ])?;
    // Each change is `:<old mode> <new mode> <old id> <new id> <status>`
    // followed by its path, both NUL-terminated.
    let mut fields = diff.split_terminator('\0');
    while let Some(change) = fields.next() {
        let path = fields
            .next()
            .with_context(|| format!("no path follows the change `{change}`"))?;
        let modified_regular_file = change.starts_with(":100644 100644 ") && change.ends_with(" M");
        if !(modified_regular_file && path.ends_with(".rs")) {
            return Ok(false);
        }
    }
    Ok(true)
}
