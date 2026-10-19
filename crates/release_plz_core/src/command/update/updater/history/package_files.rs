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

use std::iter::Peekable;

use git_cmd::{HistoryCommit, Repo};
use tracing::debug;

use crate::package_compare::PackageFiles;

/// The number of pairs of consecutive commits that the first batch of a walk
/// compares. Each later batch compares twice as many pairs as Git reported on
/// in the previous one, and at least as many as the first.
const FIRST_BATCH_SIZE: usize = 8;

/// The package files of the checkouts of a history walk.
pub(crate) struct CheckoutPackageFiles<'a> {
    repository: &'a Repo,
    /// The commits of the walk, in the order it checks out those it doesn't skip.
    walk: &'a [HistoryCommit],
    /// The comparisons of the last batch: whether only Rust sources differ
    /// between each commit of `walk` from position `batch_start` on and the
    /// next one, up to the first pair Git didn't report on.
    batch: Vec<bool>,
    /// The position in `walk` of the first commit of the last batch.
    batch_start: usize,
    /// The position in `walk` of the last checkout, whose files `files` lists.
    /// `None` before the first checkout.
    previous: Option<usize>,
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
            batch: Vec::new(),
            batch_start: 0,
            previous: None,
            prevents_reuse: None,
            files: PackageFiles::default(),
        }
    }

    /// The package files at the commit at position `index` of the walk,
    /// reusing the previous checkout's list when possible. Call this after
    /// checking that commit out, in walk order, before inspecting the package.
    pub(crate) fn files_at(&mut self, index: usize) -> &PackageFiles {
        let previous = self.previous.replace(index);
        if !previous.is_some_and(|previous| self.only_sources_differ(previous, index)) {
            self.files = PackageFiles::default();
            self.prevents_reuse = None;
            return &self.files;
        }
        let commit = &self.walk[index].id;
        // Source-only differences can't add or remove a checkout hook or a
        // symlinked manifest, so checking this checkout tells about the
        // previous one, and the answer holds until the trees differ otherwise.
        // Checking only when a list could be reused saves the Git calls at
        // checkouts whose successor differs beyond sources.
        let prevents_reuse = *self
            .prevents_reuse
            .get_or_insert_with(|| has_hook_or_symlinked_manifest(self.repository, commit));
        if prevents_reuse {
            // A hook can change files at each checkout without changing the
            // trees, and a symlinked manifest can point to a Rust source.
            self.files = PackageFiles::default();
        } else if self.files.is_cached() {
            debug!("reusing historical package file list at {commit}");
        }
        &self.files
    }

    /// Whether the trees of the checkouts at the positions `previous` and
    /// `index` of the walk differ only in Rust sources, see [`compare_trees`].
    fn only_sources_differ(&mut self, previous: usize, index: usize) -> bool {
        let walk = self.walk;
        if previous + 1 != index {
            // The walk skipped the commits in between.
            let commits = [walk[previous].id.as_str(), walk[index].id.as_str()];
            return compare_trees(self.repository, &commits) == [true];
        }
        // Most checkouts follow the commit before them in the walk: compare
        // those pairs in batches, from the current checkout on. The walk can
        // stop at any checkout, so the batches start small and double in size:
        // a batch compares about as many pairs as the walk covered before it,
        // and a long walk takes few Git calls. Git compares each pair on its
        // own, so a pair it didn't report on just starts the next batch.
        if previous - self.batch_start >= self.batch.len() {
            let size = FIRST_BATCH_SIZE.max(2 * self.batch.len());
            let end = walk.len().min(index + size);
            let commits: Vec<_> = walk[previous..end]
                .iter()
                .map(|commit| commit.id.as_str())
                .collect();
            self.batch = compare_trees(self.repository, &commits);
            self.batch_start = previous;
        }
        self.batch.get(previous - self.batch_start) == Some(&true)
    }
}

/// Whether the checkout of `commit` has, or may have, a checkout hook or a
/// symlinked manifest.
fn has_hook_or_symlinked_manifest(repository: &Repo, commit: &str) -> bool {
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

/// Whether the trees of each commit of `commits` and the next one differ only
/// in the contents of `.rs` files that are non-executable regular files in
/// both, up to the first pair Git doesn't report on.
fn compare_trees(repository: &Repo, commits: &[&str]) -> Vec<bool> {
    // With `--stdin`, `diff-tree` compares the commits of each input line
    // `<new> <old>` as if `old` were the parent of `new`, and prints `new`
    // before their changes, even without changes thanks to `--always`. It
    // prints nothing for a line it can't process, though, and still succeeds:
    // only the commits it prints in input order are known.
    let input: String = commits
        .windows(2)
        .map(|pair| format!("{} {}\n", pair[1], pair[0]))
        .collect();
    // `diff-tree` detects no renames, so it reads no blobs and reports a rename
    // as a deletion and an addition. Report submodule updates even if
    // `submodule.<name>.ignore` hides them.
    let args = [
        "diff-tree",
        "-r",
        "-z",
        "--ignore-submodules=none",
        "--always",
        "--stdin",
    ];
    let diff = match repository.git_output_with_input(&args, input.as_bytes()) {
        Ok(diff) => diff,
        Err(error) => {
            debug!("cannot compare the trees of commit pairs: {error:#}");
            return Vec::new();
        }
    };
    // With `-z`, Git prints paths verbatim, so they can be invalid UTF-8.
    let mut fields = diff.split(|byte| *byte == b'\0').peekable();
    let changes: Vec<bool> = commits
        .iter()
        .skip(1)
        .map_while(|new| next_comparison(&mut fields, new.as_bytes()))
        .collect();
    if let Some(new) = commits.get(changes.len() + 1) {
        debug!("diff-tree didn't report on {new} as expected");
    }
    changes
}

/// Read the comparison of `commit` from the `diff-tree` output `fields`: whether
/// all of its changes are [Rust source changes](is_rust_source_change). `None`
/// if `fields` doesn't continue with `commit` and its changes.
fn next_comparison<'d>(
    fields: &mut Peekable<impl Iterator<Item = &'d [u8]>>,
    commit: &[u8],
) -> Option<bool> {
    if fields.next()? != commit {
        return None;
    }
    let mut only_sources = true;
    while let Some(change) = fields.next_if(|field| field.starts_with(b":")) {
        only_sources &= is_rust_source_change(change, fields.next()?);
    }
    Some(only_sources)
}

/// Whether the `diff-tree` `change` of `path` modifies the contents of a `.rs`
/// file that is a non-executable regular file on both sides. Each change is
/// `:<old mode> <new mode> <old id> <new id> <status>`.
fn is_rust_source_change(change: &[u8], path: &[u8]) -> bool {
    let modified_regular_file = change.starts_with(b":100644 100644 ") && change.ends_with(b" M");
    modified_regular_file && path.ends_with(b".rs")
}
