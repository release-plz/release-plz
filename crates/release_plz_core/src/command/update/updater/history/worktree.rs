//! Additional checkouts of the walked repository, so that the histories of
//! several packages can be walked at the same time.
//!
//! The walked repository is a temporary copy of the project, with any
//! uncommitted changes stashed. A new worktree at the same commit shares the
//! objects, refs, configuration and hooks of the copy, but can lack its
//! submodule files and untracked files, as well as ignored files, which Cargo
//! packages when a manifest's `include` list names them. With
//! `submodule.recurse`, checkouts also use the submodule repositories in the
//! Git directory of the copy, which a worktree doesn't find.
//! Git's line-ending and attribute conversions can also change clean files'
//! bytes in a fresh checkout, while package equality compares the bytes on disk.
//! [`supports_worktrees`] rules these cases out.

use anyhow::Context as _;
use git_cmd::Repo;
use tracing::debug;

use crate::fs_utils::Utf8TempDir;

/// Whether a worktree of `repository` checks out the files Cargo packages in
/// `repository` at every commit of the walk, and creating it leaves the
/// original repository of the project untouched.
pub(crate) fn supports_worktrees(repository: &Repo) -> bool {
    match reason_against_worktrees(repository) {
        Ok(None) => true,
        Ok(Some(reason)) => {
            debug!("walking histories one at a time: {reason}");
            false
        }
        Err(error) => {
            debug!("cannot check whether worktrees can walk histories: {error:#}");
            false
        }
    }
}

/// The reason worktrees can't walk the histories of `repository`, if any.
fn reason_against_worktrees(repository: &Repo) -> anyhow::Result<Option<&'static str>> {
    // When `.git` is a file, as in linked worktrees and repositories with a
    // separate Git directory, the copy shares the Git directory of the project,
    // which would register the new worktree. Git reports a repository's own Git
    // directory relative to the working directory.
    let common_dir = repository.git(&["rev-parse", "--git-common-dir"])?;
    if !repository
        .directory()
        .join(common_dir)
        .starts_with(repository.directory())
    {
        return Ok(Some("the repository shares the project's Git directory"));
    }
    // A new worktree has the files of HEAD, with their committed contents and
    // no untracked files. It has no submodule files either, while Cargo
    // packages them. The clean check of the copy tolerates type changes, so
    // its index can differ from HEAD.
    if repository
        .git(&["diff-index", "--cached", "--quiet", "HEAD", "--"])
        .is_err()
    {
        return Ok(Some("the index differs from HEAD"));
    }
    // Git lists each indexed file as `<tag> <mode> <id> <stage>\t<path>`, with
    // mode 160000 for submodules. The tag is `H` unless the file has the
    // skip-worktree bit, such as files a sparse checkout leaves out, or the
    // assume-unchanged bit. Modified files, type changes included, are listed
    // again as `C`, and every untracked file as `?`. Include ignored files:
    // Cargo's explicit include list can package them. The copy already excludes
    // ordinary ignored files (such as target), but retains globally ignored
    // files and can contain files created by checkout hooks.
    let files = repository.git(&["ls-files", "--stage", "--modified", "--others", "-v", "-z"])?;
    let has_other_files = files.split_terminator('\0').any(|entry| {
        entry
            .strip_prefix("H ")
            .is_none_or(|entry| entry.starts_with("160000 "))
    });
    if has_other_files {
        return Ok(Some("a new worktree would have other files"));
    }
    if has_checkout_conversion(repository, &files)? {
        return Ok(Some("Git can change checkout contents"));
    }
    // With `submodule.recurse`, checking a commit out also checks its
    // submodules out, from their repositories in the Git directory. A worktree
    // looks for them in its own Git directory, so checking out a commit with a
    // submodule that HEAD no longer has fails there. Without these
    // repositories, such as in a fresh clone, checkouts behave alike in the
    // copy and in a worktree. Users can set `submodule.recurse` globally, so
    // the setting alone doesn't rule worktrees out.
    let recurse_submodules = repository.git(&[
        "config",
        "--type=bool",
        "--default=false",
        "--get",
        "submodule.recurse",
    ])?;
    if recurse_submodules == "true" && repository.git_path("modules")?.try_exists()? {
        return Ok(Some("Git recurses into submodules"));
    }
    Ok(None)
}

/// Clean files need not have the bytes a fresh checkout writes: Git compares
/// their normalized contents, whereas Cargo packages the bytes on disk. For
/// example, committing LF files with `core.autocrlf=true` leaves them as LF,
/// but a worktree writes CRLF. Keep the original checkout when conversions
/// could change those bytes, even if a particular file would be unaffected.
fn has_checkout_conversion(repository: &Repo, files: &str) -> anyhow::Result<bool> {
    let autocrlf = repository.git(&[
        "config",
        "--type=bool-or-str",
        "--default=false",
        "--get",
        "core.autocrlf",
    ])?;
    if autocrlf != "false" {
        return Ok(true);
    }

    // The earlier checks leave only indexed files. Query their effective
    // attributes, including info/attributes and global attributes. NUL
    // delimiters preserve paths containing whitespace or newlines.
    let mut paths = Vec::new();
    for entry in files.split_terminator('\0') {
        let (_, path) = entry.split_once('\t').context("indexed file has no path")?;
        paths.extend_from_slice(path.as_bytes());
        paths.push(0);
    }
    let attributes = repository.git_output_with_input(
        &[
            "check-attr",
            "-z",
            "--stdin",
            "text",
            "eol",
            "crlf",
            "filter",
            "ident",
            "working-tree-encoding",
        ],
        &paths,
    )?;
    // Each record is <path> NUL <attribute> NUL <value> NUL. Unset and
    // unspecified attributes don't enable a conversion.
    Ok(attributes
        .split(|&byte| byte == 0)
        .skip(2)
        .step_by(3)
        .any(|value| !matches!(value, b"unset" | b"unspecified")))
}

/// A worktree of a repository, detached at a commit and removed on drop.
pub(crate) struct Worktree<'a> {
    repository: &'a Repo,
    worktree: Repo,
    /// Deletes the worktree's files if Git can't remove the worktree.
    _dir: Utf8TempDir,
}

impl<'a> Worktree<'a> {
    /// Check `commit` out in a new worktree of `repository`.
    pub(crate) fn new(repository: &'a Repo, commit: &str) -> anyhow::Result<Self> {
        let dir = Utf8TempDir::new()?;
        // Name the worktree like the temporary copy of the project, so relative
        // paths that leave the repository resolve alike.
        let path = crate::new_project_root(repository.directory(), dir.path())?;
        repository.add_worktree(&path, commit)?;
        let worktree = Repo::new(&path).context("cannot open worktree")?;
        Ok(Self {
            repository,
            worktree,
            _dir: dir,
        })
    }

    pub(crate) fn repo(&self) -> &Repo {
        &self.worktree
    }
}

impl Drop for Worktree<'_> {
    fn drop(&mut self) {
        // The removal is forced, as a failed walk can leave changes behind.
        let path = self.worktree.directory();
        if let Err(error) = self.repository.force_remove_worktree(path) {
            // The worktree belongs to a temporary repository, so a stale entry
            // is harmless.
            debug!("cannot remove worktree {path}: {error:#}");
        }
    }
}
