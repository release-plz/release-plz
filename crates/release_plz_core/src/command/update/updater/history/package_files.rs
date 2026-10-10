//! Reuse a package's file list across the checkouts of a history walk.
//!
//! The walk checks commits out in a clean temporary copy of the repository, so
//! the tracked files on disk are those of the checked-out commit, and
//! untracked and ignored files change only at checkouts that add or remove
//! tracked paths. Cargo selects packaged files by their paths and types,
//! manifests, ignore rules and configuration, never by the contents of Rust
//! sources. Checkouts whose trees differ only in the contents of `.rs` files
//! that are non-executable regular files in both therefore package the same
//! files: symlinks and submodules stand for other files, and executable scripts
//! can be Cargo or rustc wrappers. Such differences are transitive, so
//! comparing each checkout with the previous one covers every checkout since
//! Cargo listed the files.
//!
//! A symlinked file-selection input pointing to a `.rs` file makes its contents
//! matter, though. Only symlinked manifests are detected: Cargo lists the files
//! at each checkout with a symlink named `Cargo.toml` in any ASCII case, since
//! case-insensitive filesystems resolve every casing. Source-only differences
//! preserve their absence, so only checkouts that need a new list are inspected.
//! Other symlinked inputs pointing to Rust sources, such as a `.gitignore` or a
//! `.cargo/config.toml`, are assumed absent, and configuration outside the
//! repository is assumed stable during a walk, as for the released-package
//! cache. Whenever Git cannot compare the checkouts or look for symlinked
//! manifests, Cargo lists the files again.

use anyhow::Context as _;
use git_cmd::Repo;
use tracing::debug;

use crate::package_compare::PackageFiles;

/// The package files of the checkouts of a history walk.
pub(crate) struct CheckoutPackageFiles<'a> {
    repository: &'a Repo,
    /// The last checked-out commit, whose files `files` lists. `None` before the
    /// first checkout and when that commit has, or may have, a symlinked
    /// manifest.
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

    /// The package files at `commit`, reusing the previous checkout's list when
    /// possible. Call this after checking `commit` out, before inspecting the
    /// package.
    pub(crate) fn files_at(&mut self, commit: &str) -> &PackageFiles {
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
        let cacheable = reuses
            || has_symlinked_manifest(self.repository)
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

/// Whether the checked-out commit has a symlink named `Cargo.toml`, in any
/// ASCII case, in any directory.
fn has_symlinked_manifest(repository: &Repo) -> anyhow::Result<bool> {
    // After `git checkout <commit>` in the walk's clean copy, the index holds the
    // tree of that commit. Unlike `ls-tree`, `ls-files` accepts this pathspec magic.
    let manifests = repository.git(&[
        "ls-files",
        "--stage",
        "-z",
        "--",
        ":(glob,icase)**/Cargo.toml",
    ])?;
    // Each entry is `<mode> <id> <stage>\t<path>`, NUL-terminated.
    Ok(manifests
        .split_terminator('\0')
        .any(|entry| entry.starts_with("120000 ")))
}

/// Whether the trees of the commits `old` and `new` differ only in the contents
/// of `.rs` files that are non-executable regular files in both.
fn only_rust_sources_differ(repository: &Repo, old: &str, new: &str) -> anyhow::Result<bool> {
    // `diff-tree` detects no renames, so it reads no blobs and reports a rename
    // as a deletion and an addition. Report submodule updates even if
    // `submodule.<name>.ignore` hides them.
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
