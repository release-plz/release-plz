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
    // With core.symlinks=false, Git can consider an existing real symlink
    // clean even though a fresh checkout writes the link target as plain text.
    if files
        .split_terminator('\0')
        .any(|entry| entry.starts_with("H 120000 "))
        && !checks_out_symlinks(repository)?
    {
        return Ok(Some("Git checks out symlinks as plain files"));
    }
    if has_checkout_conversion(repository)? {
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

fn checks_out_symlinks(repository: &Repo) -> anyhow::Result<bool> {
    Ok(repository.git(&[
        "config",
        "--type=bool",
        "--default=true",
        "--get",
        "core.symlinks",
    ])? == "true")
}

/// Clean files need not have the bytes a fresh checkout writes: Git compares
/// their normalized contents, whereas Cargo packages the bytes on disk. For
/// example, committing LF files with `core.autocrlf=true` leaves them as LF,
/// but a worktree writes CRLF. Keep the original checkout when conversions
/// could change those bytes, even if a particular file would be unaffected.
fn has_checkout_conversion(repository: &Repo) -> anyhow::Result<bool> {
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

    // Query the effective attributes of the indexed files, including
    // info/attributes and global attributes. NUL delimiters preserve paths
    // containing whitespace or newlines.
    let paths = repository.git_output(&["ls-files", "-z"])?;
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
        let checkout = Self {
            repository,
            worktree,
            _dir: dir,
        };
        // Conditional includes and worktree-specific config can give a linked
        // checkout different rules. Reject it before its worker takes any jobs.
        anyhow::ensure!(
            !has_checkout_conversion(checkout.repo())?
                && checks_out_symlinks(checkout.repo())? == checks_out_symlinks(repository)?,
            "Git can change checkout contents in the linked worktree"
        );
        Ok(checkout)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(dir: &Utf8TempDir) -> Repo {
        let repo = Repo::init(dir.path());
        repo.git(&["config", "core.autocrlf", "false"]).unwrap();
        // Override global attributes that could independently disable worktrees.
        fs_err::create_dir_all(dir.path().join(".git/info")).unwrap();
        fs_err::write(
            dir.path().join(".git/info/attributes"),
            "* -text -eol -crlf -filter -ident -working-tree-encoding\n",
        )
        .unwrap();
        repo
    }

    #[cfg(unix)]
    #[test]
    fn indexed_symlink_requires_a_checkout_that_preserves_links() {
        let dir = Utf8TempDir::new().unwrap();
        let repo = repository(&dir);
        repo.git(&["config", "core.symlinks", "true"]).unwrap();
        std::os::unix::fs::symlink("README.md", dir.path().join("link")).unwrap();
        repo.add_all_and_commit("add symlink").unwrap();
        assert!(supports_worktrees(&repo));

        // Changing the setting leaves the link in place and the index clean.
        repo.git(&["config", "core.symlinks", "false"]).unwrap();
        repo.is_clean().unwrap();
        assert_eq!(
            reason_against_worktrees(&repo).unwrap(),
            Some("Git checks out symlinks as plain files")
        );
    }

    #[test]
    fn linked_checkout_checks_its_own_conversion_settings() {
        let dir = Utf8TempDir::new().unwrap();
        let repo = repository(&dir);
        repo.git(&["config", "core.autocrlf", "true"]).unwrap();
        // Only the main checkout has a Git directory ending in .git.
        fs_err::write(
            dir.path().join(".git/main-config"),
            "[core]\n autocrlf = false\n",
        )
        .unwrap();
        repo.git(&["config", "includeIf.gitdir:**/.git.path", "main-config"])
            .unwrap();
        assert!(supports_worktrees(&repo));

        let error = Worktree::new(&repo, "HEAD")
            .err()
            .expect("linked checkout should be rejected");
        assert!(
            error
                .to_string()
                .contains("Git can change checkout contents"),
            "{error:#}"
        );
        // The rejected worktree must be unregistered as well as removed from disk.
        assert_eq!(
            repo.git(&["worktree", "list", "--porcelain"])
                .unwrap()
                .lines()
                .filter(|line| line.starts_with("worktree "))
                .count(),
            1
        );
    }
}
