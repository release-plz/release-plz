//! Additional checkouts of the walked repository, so that the histories of
//! several packages can be walked at the same time.
//!
//! The walked repository is a temporary copy of the project, with any
//! uncommitted changes stashed. A new worktree at the same commit shares the
//! objects, refs, configuration and hooks of the copy, and usually has the same
//! files. Cargo can also package ignored files through a manifest's include
//! list. [`supports_worktrees`] rules out the differences that Cargo can see.

use anyhow::Context as _;
use git_cmd::Repo;
use tracing::debug;

use crate::fs_utils::Utf8TempDir;

/// Whether a worktree of `repository` checks out the files Cargo packages in
/// `repository`, and creating it leaves the original repository of the
/// project untouched.
pub(crate) fn supports_worktrees(repository: &Repo) -> bool {
    worktree_support(repository).unwrap_or_else(|error| {
        debug!("cannot check whether worktrees can walk histories: {error:#}");
        false
    })
}

fn worktree_support(repository: &Repo) -> anyhow::Result<bool> {
    // The copy of a linked worktree shares the Git directory of the original
    // repository, which would register the new worktree. Git reports a
    // repository's own Git directory relative to the working directory.
    let common_dir = repository.git(&["rev-parse", "--git-common-dir"])?;
    if !repository
        .directory()
        .join(common_dir)
        .starts_with(repository.directory())
    {
        debug!("walking histories one at a time: the repository is a linked worktree");
        return Ok(false);
    }
    // A new worktree has the files of HEAD, with their committed contents and
    // no untracked files. It has no submodule files either, while Cargo
    // packages them. The clean check of the copy tolerates type changes, so
    // its index can differ from HEAD.
    if repository
        .git(&["diff-index", "--cached", "--quiet", "HEAD", "--"])
        .is_err()
    {
        debug!("walking histories one at a time: the index differs from HEAD");
        return Ok(false);
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
        debug!("walking histories one at a time: a new worktree would have other files");
        return Ok(false);
    }
    // With `submodule.recurse`, checking a commit out also checks its
    // submodules out, from their repositories in the Git directory. A worktree
    // looks for them in its own Git directory, so checking out a commit with a
    // submodule that HEAD no longer has fails there.
    let recurse_submodules = repository.git(&[
        "config",
        "--type=bool",
        "--default=false",
        "--get",
        "submodule.recurse",
    ])?;
    if recurse_submodules == "true" {
        debug!("walking histories one at a time: Git recurses into submodules");
        return Ok(false);
    }
    Ok(true)
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
        // Keep the directory name, as the temporary copy of the project does, so
        // relative paths that leave the repository resolve alike.
        let name = repository
            .directory()
            .file_name()
            .context("the repository directory has no name")?;
        let path = dir.path().join(name);
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
        // Force the removal: a failed walk can leave changes behind.
        let path = self.worktree.directory().as_str();
        if let Err(error) = self
            .repository
            .git(&["worktree", "remove", "--force", path])
        {
            // The worktree belongs to a temporary repository, so a stale entry
            // is harmless.
            debug!("cannot remove worktree {path}: {error:#}");
        }
    }
}
