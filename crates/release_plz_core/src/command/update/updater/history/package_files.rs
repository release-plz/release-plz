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
//! `post-checkout`, `post-index-change` or `reference-transaction` hook, in the
//! hooks directory or in the Git configuration, or with a symlink named
//! `Cargo.toml` in any ASCII case. Other symlinked inputs pointing to Rust
//! sources, such as a `.gitignore` or a `.cargo/config.toml`, are assumed
//! absent, as is Git configuration included from a `.rs` file. Hooks and
//! configuration outside the trees are assumed stable during a walk.

use std::collections::HashMap;

use anyhow::Context as _;
use git_cmd::{HistoryCommit, Repo};
use tracing::debug;

use crate::package_compare::PackageFiles;

/// The package files of the checkouts of a history walk.
pub(crate) struct CheckoutPackageFiles<'a> {
    repository: &'a Repo,
    /// The commits of the walk, in the order it checks out those it doesn't skip.
    walk: &'a [HistoryCommit],
    /// Each commit of `walk` after the first, mapped to the commit before it and
    /// to whether only Rust sources differ between the two. `None` until the
    /// second checkout.
    consecutive_changes: Option<HashMap<&'a str, (&'a str, bool)>>,
    /// The last checked-out commit, whose files `files` lists. `None` before the
    /// first checkout.
    commit: Option<String>,
    /// Whether the last checkout has, or may have, a checkout hook or a
    /// symlinked manifest. `None` until a checkout that could reuse `files`
    /// needs to know.
    prevents_reuse: Option<bool>,
    files: PackageFiles,
}

impl<'a> CheckoutPackageFiles<'a> {
    /// Prepare to check out the commits of `walk` in `repository`.
    pub(crate) fn new(repository: &'a Repo, walk: &'a [HistoryCommit]) -> Self {
        Self {
            repository,
            walk,
            consecutive_changes: None,
            commit: None,
            prevents_reuse: None,
            files: PackageFiles::default(),
        }
    }

    /// The package files at `commit`, reusing the previous checkout's list when
    /// possible. Call this after checking `commit` out, before inspecting the
    /// package.
    pub(crate) fn files_at(&mut self, commit: &str) -> &PackageFiles {
        let previous = self.commit.replace(commit.to_owned());
        let only_sources_differ =
            previous.is_some_and(|previous| self.only_rust_sources_differ(&previous, commit));
        if !only_sources_differ {
            self.files = PackageFiles::default();
            self.prevents_reuse = None;
            return &self.files;
        }
        // Source-only differences can't add or remove a checkout hook or a
        // symlinked manifest, so checking this checkout tells about the
        // previous one, and the answer holds until the trees differ otherwise.
        // Checking only when a list could be reused saves the Git calls at
        // checkouts whose successor differs beyond sources.
        let prevents_reuse = *self
            .prevents_reuse
            .get_or_insert_with(|| prevents_reuse(self.repository, commit));
        if prevents_reuse {
            // A hook can change files at each checkout without changing the
            // trees, and a symlinked manifest can point to a Rust source.
            self.files = PackageFiles::default();
        } else if self.files.is_cached() {
            debug!("reusing historical package file list at {commit}");
        }
        &self.files
    }

    /// Whether the trees of the checkouts `previous` and `commit` differ only in
    /// Rust sources, see [`only_rust_sources_differ`].
    fn only_rust_sources_differ(&mut self, previous: &str, commit: &str) -> bool {
        // Most checkouts follow the commit before them in the walk: compare all
        // of those pairs with one Git call. The walk may skip commits, though.
        let consecutive_changes = self.consecutive_changes.get_or_insert_with(|| {
            consecutive_changes(self.repository, self.walk).unwrap_or_else(|error| {
                debug!("cannot compare the trees of consecutive commits: {error:#}");
                HashMap::new()
            })
        });
        match consecutive_changes.get(commit) {
            Some(&(before, only_sources)) if before == previous => only_sources,
            _ => only_rust_sources_differ(self.repository, previous, commit).unwrap_or_else(
                |error| {
                    debug!("cannot compare the trees of {previous} and {commit}: {error:#}");
                    false
                },
            ),
        }
    }
}

/// Whether the checkout of `commit` has, or may have, a checkout hook or a
/// symlinked manifest.
fn prevents_reuse(repository: &Repo, commit: &str) -> bool {
    let has_hook = has_checkout_hook(repository).unwrap_or_else(|error| {
        debug!("cannot check for checkout hooks at {commit}: {error:#}");
        true
    });
    has_hook
        || has_symlinked_manifest(repository).unwrap_or_else(|error| {
            debug!("cannot check for symlinked manifests at {commit}: {error:#}");
            true
        })
}

/// The hooks that `git checkout` runs.
const CHECKOUT_HOOKS: [&str; 3] = [
    "post-checkout",
    "post-index-change",
    "reference-transaction",
];

/// Whether a hook that `git checkout` runs may have changed this checkout's
/// packaged files.
fn has_checkout_hook(repository: &Repo) -> anyhow::Result<bool> {
    Ok(has_hook_file(repository)? || has_configured_hook(repository))
}

/// Whether the hooks directory has one of the [`CHECKOUT_HOOKS`].
fn has_hook_file(repository: &Repo) -> anyhow::Result<bool> {
    // Let Git resolve core.hooksPath, including paths relative to the worktree.
    // Git reports the directory relative to the repository directory unless it
    // is absolute. Git for Windows ignores the executable bit of hooks, so test
    // existence only.
    let hooks = repository.git(&["rev-parse", "--git-path", "hooks"])?;
    let hooks = repository.directory().join(hooks);
    for hook in CHECKOUT_HOOKS {
        let hook = hooks.join(hook);
        if fs_err::exists(&hook)? {
            return Ok(true);
        }
        // Git for Windows also runs `<hook>.exe`.
        if cfg!(windows) && fs_err::exists(hook.with_extension("exe"))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the Git configuration defines a hook for one of the
/// [`CHECKOUT_HOOKS`], as Git 2.54 and later allow. Older Git ignores these
/// definitions.
fn has_configured_hook(repository: &Repo) -> bool {
    // The command fails without a match. The checkout that just ran read the
    // same configuration, so other failures are not expected. Querying only the
    // matching entries keeps the rest of the configuration, which can hold
    // credentials, out of trace logs.
    let events = format!("^({})$", CHECKOUT_HOOKS.join("|"));
    repository
        .git(&["config", "--get-regexp", r"^hook\..*\.event$", &events])
        .is_ok()
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

/// `git diff-tree` with the options that [`only_rust_sources_differ`] needs.
/// `diff-tree` detects no renames, so it reads no blobs and reports a rename
/// as a deletion and an addition. Report submodule updates even if
/// `submodule.<name>.ignore` hides them.
const DIFF_TREE: [&str; 4] = ["diff-tree", "-r", "-z", "--ignore-submodules=none"];

/// Whether the trees of the commits `old` and `new` differ only in the contents
/// of `.rs` files that are non-executable regular files in both.
fn only_rust_sources_differ(repository: &Repo, old: &str, new: &str) -> anyhow::Result<bool> {
    let diff = repository.git(&[DIFF_TREE.as_slice(), &[old, new]].concat())?;
    let mut fields = diff.split_terminator('\0');
    while let Some(change) = fields.next() {
        let path = fields
            .next()
            .with_context(|| format!("no path follows the change `{change}`"))?;
        if !is_rust_source_change(change, path) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// [`only_rust_sources_differ`] for each commit of `walk` after the first and
/// the commit before it, keyed by the former and paired with the latter.
fn consecutive_changes<'a>(
    repository: &Repo,
    walk: &'a [HistoryCommit],
) -> anyhow::Result<HashMap<&'a str, (&'a str, bool)>> {
    let mut changes: HashMap<_, _> = walk
        .windows(2)
        .map(|pair| (pair[1].id.as_str(), (pair[0].id.as_str(), true)))
        .collect();
    if changes.is_empty() {
        return Ok(changes);
    }
    // With `--stdin`, `diff-tree` compares the commits of each input line
    // `<commit> <parent>` as if `parent` were the parent of `commit`, and
    // prints `commit` before their changes, if any.
    let input: String = changes
        .iter()
        .map(|(commit, (before, _))| format!("{commit} {before}\n"))
        .collect();
    let diff = repository.git_with_input(&[DIFF_TREE.as_slice(), &["--stdin"]].concat(), &input)?;
    let mut commit = None;
    let mut fields = diff.split_terminator('\0');
    while let Some(field) = fields.next() {
        if !field.starts_with(':') {
            commit = Some(field);
            continue;
        }
        let path = fields
            .next()
            .with_context(|| format!("no path follows the change `{field}`"))?;
        let commit = commit.with_context(|| format!("no commit precedes the change `{field}`"))?;
        let (_, only_sources) = changes
            .get_mut(commit)
            .with_context(|| format!("unexpected commit `{commit}`"))?;
        *only_sources &= is_rust_source_change(field, path);
    }
    Ok(changes)
}

/// Whether the `diff-tree` `change` of `path` modifies the contents of a `.rs`
/// file that is a non-executable regular file on both sides. Each change is
/// `:<old mode> <new mode> <old id> <new id> <status>`.
fn is_rust_source_change(change: &str, path: &str) -> bool {
    let modified_regular_file = change.starts_with(":100644 100644 ") && change.ends_with(" M");
    modified_regular_file && path.ends_with(".rs")
}
