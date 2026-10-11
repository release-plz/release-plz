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
//! Hooks that run at checkout can change files without changing the trees,
//! though, and a symlinked file-selection input pointing to a `.rs` file makes
//! its contents matter. Cargo lists the files at each checkout with a
//! `post-checkout`, `post-index-change` or `reference-transaction` hook, or
//! with a symlink named `Cargo.toml` in any ASCII case. Other symlinked inputs
//! pointing to Rust sources, such as a `.gitignore` or a `.cargo/config.toml`,
//! are assumed absent, and configuration outside the repository is assumed
//! stable during a walk.

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
        // A hook that ran at this checkout can have changed files without
        // changing the trees. Check at every checkout: a tracked hook can be
        // absent at HEAD but present in older snapshots.
        let has_hook = has_checkout_hook(self.repository).unwrap_or_else(|error| {
            debug!("cannot check for checkout hooks at {commit}: {error:#}");
            true
        });
        let previous = self.commit.take().filter(|_| !has_hook);
        let reuses = previous.is_some_and(|previous| {
            only_rust_sources_differ(self.repository, &previous, commit).unwrap_or_else(|error| {
                debug!("cannot compare the trees of {previous} and {commit}: {error:#}");
                false
            })
        });
        if reuses {
            // The previous checkout had no symlinked manifest, and source-only
            // differences cannot add one.
            if self.files.is_cached() {
                debug!("reusing historical package file list at {commit}");
            }
        } else {
            self.files = PackageFiles::default();
            let has_symlinked_manifest =
                has_symlinked_manifest(self.repository).unwrap_or_else(|error| {
                    debug!("cannot check for symlinked manifests at {commit}: {error:#}");
                    true
                });
            if has_symlinked_manifest {
                return &self.files;
            }
        }
        self.commit = Some(commit.to_owned());
        &self.files
    }
}

/// Whether a hook that `git checkout` runs may have changed this checkout's
/// packaged files.
fn has_checkout_hook(repository: &Repo) -> anyhow::Result<bool> {
    // Let Git resolve core.hooksPath, including paths relative to the worktree.
    // Git reports each path on its own line, relative to the repository
    // directory unless it is absolute. Git for Windows ignores the executable
    // bit of hooks, so test existence only.
    let hooks = repository.git(&[
        "rev-parse",
        "--git-path",
        "hooks/post-checkout",
        "--git-path",
        "hooks/post-index-change",
        "--git-path",
        "hooks/reference-transaction",
    ])?;
    for hook in hooks.lines() {
        if fs_err::exists(repository.directory().join(hook))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the checked-out commit has a symlink named `Cargo.toml`, in any
/// ASCII case, in any directory.
fn has_symlinked_manifest(repository: &Repo) -> anyhow::Result<bool> {
    // After `git checkout <commit>` in the walk's clean copy, the index holds the
    // tree of that commit. Unlike `ls-tree`, `ls-files` accepts this pathspec magic.
    // `icase` matches every casing, since case-insensitive filesystems resolve
    // any of them to the manifest.
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
