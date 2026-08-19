//! Run git as shell shell and parse its stdout.

mod cmd;
#[cfg(feature = "test_fixture")]
pub mod test_fixture;

use std::{collections::HashSet, path::Path, process::Command};

use anyhow::{Context, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use tracing::{Span, debug, instrument, trace, warn};

/// Repository
#[derive(Debug)]
pub struct Repo {
    /// Directory where you want to run git operations
    directory: Utf8PathBuf,
    /// Branch name before running any git operation
    original_branch: String,
    /// Remote name before running any git operation
    original_remote: String,
}

impl Repo {
    /// Returns an error if the directory doesn't contain any commit
    #[instrument(skip_all)]
    pub fn new(directory: impl AsRef<Utf8Path>) -> anyhow::Result<Self> {
        debug!("initializing directory {:?}", directory.as_ref());

        let (current_remote, current_branch) = Self::get_current_remote_and_branch(&directory)
            .context("cannot determine current branch")?;

        Ok(Self {
            directory: directory.as_ref().to_path_buf(),
            original_branch: current_branch,
            original_remote: current_remote,
        })
    }

    pub fn directory(&self) -> &Utf8Path {
        &self.directory
    }

    fn get_current_remote_and_branch(
        directory: impl AsRef<Utf8Path>,
    ) -> anyhow::Result<(String, String)> {
        match git_in_dir(
            directory.as_ref(),
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        ) {
            Ok(output) => output
                .split_once('/')
                .map(|(remote, branch)| (remote.to_string(), branch.to_string()))
                .context("cannot determine current remote and branch"),

            Err(e) => {
                let err = e.to_string();
                if err.contains("fatal: no upstream configured for branch") {
                    let branch = get_current_branch(directory)?;
                    warn!("no upstream configured for branch {branch}");
                    Ok(("origin".to_string(), branch))
                } else if err.contains("fatal: ambiguous argument 'HEAD': unknown revision or path not in the working tree.") {
                    Err(anyhow!("git repository does not contain any commit."))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Check if there are uncommitted changes.
    pub fn is_clean(&self) -> anyhow::Result<()> {
        let changes = self.changes_except_typechanges()?;
        anyhow::ensure!(
            changes.is_empty(),
            "the working directory of this project has uncommitted changes. If these files are both committed and in .gitignore, either delete them or remove them from .gitignore.\n\
             If the list includes submodules, run `git ls-files -ci --exclude-standard` in the submodule to check if files in the submodule are both committed and in .gitignore.\n\
             Otherwise, please commit or stash these changes:\n{changes:?}"
        );
        Ok(())
    }

    pub fn checkout_new_branch(&self, branch: &str) -> anyhow::Result<()> {
        self.git(&["checkout", "-b", branch])?;
        Ok(())
    }

    pub fn delete_branch_in_remote(&self, branch: &str) -> anyhow::Result<()> {
        self.push(&format!(":refs/heads/{branch}"))
            .with_context(|| format!("can't delete temporary branch {branch}"))
    }

    pub fn add_all_and_commit(&self, message: &str) -> anyhow::Result<()> {
        self.git(&["add", "."])?;
        self.git(&["commit", "-m", message])?;
        Ok(())
    }

    pub fn disable_gpg_signing(&self) -> anyhow::Result<()> {
        self.git(&["config", "commit.gpgsign", "false"])
            .context("failed to disable commit gpg signing")?;
        self.git(&["config", "tag.gpgsign", "false"])
            .context("failed to disable tag gpg signing")?;
        Ok(())
    }

    /// Get the list of changed files.
    /// `filter` is applied for each line of `git status --porcelain`.
    /// Only changes for which `filter` returns true are returned.
    pub fn changes(&self, filter: impl FnMut(&&str) -> bool) -> anyhow::Result<Vec<String>> {
        let output = self.git(&["status", "--porcelain"])?;
        let changed_files = changed_files(&output, filter);
        Ok(changed_files)
    }

    /// Get files changed in the current commit
    pub fn files_of_current_commit(&self) -> anyhow::Result<HashSet<Utf8PathBuf>> {
        let output = self.git(&["show", "--oneline", "--name-only", "--pretty=format:"])?;
        let changed_files = output
            .lines()
            .map(|l| l.trim())
            .map(Utf8PathBuf::from)
            .collect();
        Ok(changed_files)
    }

    pub fn changes_except_typechanges(&self) -> anyhow::Result<Vec<String>> {
        self.changes(|line| !line.starts_with("T "))
    }

    pub fn add<T: AsRef<str>>(&self, paths: &[T]) -> anyhow::Result<()> {
        let mut args = vec!["add"];
        let paths: Vec<&str> = paths.iter().map(|p| p.as_ref()).collect();
        args.extend(paths);
        self.git(&args)?;
        Ok(())
    }

    pub fn commit(&self, message: &str) -> anyhow::Result<()> {
        self.git(&["commit", "-m", message])?;
        Ok(())
    }

    pub fn commit_signed(&self, message: &str) -> anyhow::Result<()> {
        self.git(&["commit", "-s", "-m", message])?;
        Ok(())
    }

    pub fn push(&self, obj: &str) -> anyhow::Result<()> {
        self.git(&["push", &self.original_remote, obj])?;
        Ok(())
    }

    pub fn fetch(&self, obj: &str) -> anyhow::Result<()> {
        self.git(&["fetch", &self.original_remote, obj])
            .with_context(|| format!("failed to fetch {obj}"))?;
        Ok(())
    }

    pub fn force_push(&self, obj: &str) -> anyhow::Result<()> {
        // `--force-with-lease` is safer than `--force` because it will not overwrite
        // changes on the remote that you do not have locally.
        // In other words, it will only push if no one else has pushed changes to the remote
        // branch since you last pulled. If someone else has pushed changes, the command will fail,
        // preventing you from accidentally overwriting someone else's work.
        self.git(&["push", &self.original_remote, obj, "--force-with-lease"])
            .with_context(|| format!("failed to force-push {obj}"))?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn checkout_head(&self) -> anyhow::Result<()> {
        self.checkout(&self.original_branch)?;
        Ok(())
    }

    /// Branch name before running any git operation.
    /// I.e. when the [`Repo`] was created.
    pub fn original_branch(&self) -> &str {
        &self.original_branch
    }

    /// Run a git command in the repository git directory
    pub fn git(&self, args: &[&str]) -> anyhow::Result<String> {
        git_in_dir(&self.directory, args)
    }

    pub fn stash_pop(&self) -> anyhow::Result<()> {
        self.git(&["stash", "pop"])?;
        Ok(())
    }

    /// Checkout to the latest commit.
    pub fn checkout_last_commit_at_paths(&self, paths: &[&Path]) -> anyhow::Result<()> {
        let previous_commit = self.last_commit_at_paths(paths)?;
        self.checkout(&previous_commit)?;
        Ok(())
    }

    fn last_commit_at_paths(&self, paths: &[&Path]) -> anyhow::Result<String> {
        self.nth_commit_at_paths(1, paths)
            .context("failed to get message of last commit")
    }

    fn previous_commit_at_paths(&self, paths: &[&Path]) -> anyhow::Result<String> {
        self.nth_commit_at_paths(2, paths)
            .context("failed to get message of previous commit")
    }

    pub fn checkout_previous_commit_at_paths(&self, paths: &[&Path]) -> anyhow::Result<()> {
        let commit = self.previous_commit_at_paths(paths)?;
        self.checkout(&commit)?;
        Ok(())
    }

    /// Hashes of the commits that modified `paths`, from newest (`HEAD`) to oldest,
    /// excluding any commit reachable from one of `boundary_commits` (typically the git tag
    /// and/or the commit at which the last version was published).
    ///
    /// Bounding the walk by the last release — rather than walking commit by commit and
    /// stopping at the first already-released commit — matters in the presence of merge
    /// commits: a commit-by-commit walk can descend into a branch that forked *before* the
    /// last release and rejoin already-released history, stopping early and dropping the
    /// commits that landed on the main line after the release (release-plz#2494).
    ///
    /// `head` must be the branch tip the changelog is being computed for (not necessarily the
    /// currently checked-out commit). Only boundary commits that are ancestors of `head` are
    /// used: a commit that isn't reachable from `head` (a newer release living on another
    /// branch, a hash missing from a shallow clone, or simply the tag of a package unchanged
    /// since before `head`) isn't a point `head` was released at, and excluding it would
    /// wrongly drop the commits it shares with `head`. At most `max_commits` are returned.
    pub fn commits_at_paths_since(
        &self,
        head: &str,
        boundary_commits: &[&str],
        paths: &[&Path],
        max_commits: u32,
    ) -> anyhow::Result<Vec<String>> {
        let excludes: Vec<String> = boundary_commits
            .iter()
            .filter(|commit| self.is_ancestor(commit, head))
            .map(|commit| format!("^{commit}"))
            .collect();
        // `u32::MAX` means "no limit"; git's `--max-count` doesn't accept a value that large.
        let max_commits = (max_commits != u32::MAX).then(|| max_commits.to_string());

        let mut git_args = vec!["log", "--format=%H"];
        if let Some(max_commits) = &max_commits {
            git_args.push("--max-count");
            git_args.push(max_commits.as_str());
        }
        git_args.push(head);
        git_args.extend(excludes.iter().map(String::as_str));
        git_args.push("--");
        for p in paths {
            git_args.push(p.to_str().expect("invalid path"));
        }
        let commit_list = self.git(&git_args)?;
        Ok(commit_list.lines().map(|c| c.to_string()).collect())
    }

    #[instrument(skip(self))]
    pub fn checkout(&self, object: &str) -> anyhow::Result<()> {
        self.git(&["checkout", object])
            .context("failed to checkout")?;
        Ok(())
    }

    /// Adds a detached git worktree at the given path checked out at the given object.
    pub fn add_worktree(&self, path: impl AsRef<str>, object: &str) -> anyhow::Result<()> {
        self.git(&["worktree", "add", "--detach", path.as_ref(), object])
            .context("failed to create git worktree")?;

        Ok(())
    }

    /// Removes a worktree that was created for this repository at the given path.
    pub fn remove_worktree(&self, path: impl AsRef<str>) -> anyhow::Result<()> {
        self.git(&["worktree", "remove", path.as_ref()])
            .context("failed to remove worktree")?;

        Ok(())
    }

    /// Get `nth` commit starting from `1`.
    #[instrument(
        skip(self)
        fields(
            nth_commit = tracing::field::Empty,
        )
    )]
    fn nth_commit_at_paths(&self, nth: usize, paths: &[&Path]) -> anyhow::Result<String> {
        let nth_str = nth.to_string();

        let git_args = {
            let mut git_args = vec!["log", "--format=%H", "-n", &nth_str, "--"];
            for p in paths {
                let path = p.to_str().expect("invalid path");
                git_args.push(path);
            }
            git_args
        };

        let commit_list = self.git(&git_args)?;
        let mut commits = commit_list.lines();
        let last_commit = commits.nth(nth - 1).context("not enough commits")?;

        Span::current().record("nth_commit", last_commit);
        debug!("nth_commit found");
        Ok(last_commit.to_string())
    }

    pub fn current_commit_message(&self) -> anyhow::Result<String> {
        self.git(&["log", "-1", "--pretty=format:%B"])
    }

    pub fn get_author_name(&self, commit_hash: &str) -> anyhow::Result<String> {
        self.get_commit_info("%an", commit_hash)
    }

    pub fn get_author_email(&self, commit_hash: &str) -> anyhow::Result<String> {
        self.get_commit_info("%ae", commit_hash)
    }

    pub fn get_committer_name(&self, commit_hash: &str) -> anyhow::Result<String> {
        self.get_commit_info("%cn", commit_hash)
    }

    pub fn get_committer_email(&self, commit_hash: &str) -> anyhow::Result<String> {
        self.get_commit_info("%ce", commit_hash)
    }

    fn get_commit_info(&self, info: &str, commit_hash: &str) -> anyhow::Result<String> {
        self.git(&["log", "-1", &format!("--pretty=format:{info}"), commit_hash])
    }

    /// Get the SHA1 of the current HEAD.
    pub fn current_commit_hash(&self) -> anyhow::Result<String> {
        self.git(&["log", "-1", "--pretty=format:%H"])
            .context("can't determine current commit hash")
    }

    /// Create a git tag (annotated)
    pub fn tag(&self, name: &str, message: &str) -> anyhow::Result<String> {
        self.git(&["tag", "-m", message, name])
    }

    /// Create a lightweight git tag (no message, just a reference to a commit)
    pub fn tag_lightweight(&self, name: &str) -> anyhow::Result<String> {
        self.git(&["tag", name])
    }

    /// Get the commit hash of the given tag
    pub fn get_tag_commit(&self, tag: &str) -> Option<String> {
        self.git(&["rev-list", "-n", "1", tag]).ok()
    }

    /// Returns all the tags in the repository in an unspecified order.
    pub fn get_all_tags(&self) -> Vec<String> {
        match self
            .git(&["tag", "--list"])
            .ok()
            .as_ref()
            .map(|output| output.trim())
        {
            None | Some("") => vec![],
            Some(output) => output.lines().map(|line| line.to_owned()).collect(),
        }
    }

    /// Check if a commit comes before another one.
    ///
    /// ## Example
    ///
    /// For this git log:
    /// ```txt
    /// commit d6ec399b80d44bf9c4391e4a9ead8482faa9bffd
    /// commit e880d8786cb16aa9a3f258e7503932445d708df7
    /// ```
    ///
    /// `git.is_ancestor("e880d8786cb16aa9a3f258e7503932445d708df7", "d6ec399b80d44bf9c4391e4a9ead8482faa9bffd")` returns true.
    pub fn is_ancestor(&self, maybe_ancestor_commit: &str, descendant_commit: &str) -> bool {
        self.git(&[
            "merge-base",
            "--is-ancestor",
            maybe_ancestor_commit,
            descendant_commit,
        ])
        .is_ok()
    }

    /// Name of the remote when the [`Repo`] was created.
    pub fn original_remote(&self) -> &str {
        &self.original_remote
    }

    /// Url of the remote when the [`Repo`] was created.
    pub fn original_remote_url(&self) -> anyhow::Result<String> {
        let param = format!("remote.{}.url", self.original_remote);
        self.git(&["config", "--get", &param])
    }

    pub fn tag_exists(&self, tag: &str) -> anyhow::Result<bool> {
        let output = self
            .git(&["tag", "-l", tag])
            .context("cannot determine if git tag exists")?;
        Ok(output.lines().count() >= 1)
    }

    pub fn get_branches_of_commit(&self, commit_hash: &str) -> anyhow::Result<Vec<String>> {
        let output = self.git(&["branch", "--contains", commit_hash])?;
        let branches = output
            .lines()
            .filter_map(|l| l.split_whitespace().last())
            .map(|s| s.to_string())
            .collect();
        Ok(branches)
    }
}

pub fn is_file_ignored(repo_path: &Utf8Path, file: &Utf8Path) -> bool {
    let file = file.as_str();

    git_in_dir(repo_path, &["check-ignore", "--no-index", file]).is_ok()
}

pub fn is_file_committed(repo_path: &Utf8Path, file: &Utf8Path) -> bool {
    let file = file.as_str();
    git_in_dir(repo_path, &["ls-files", "--error-unmatch", file]).is_ok()
}

fn changed_files(output: &str, filter: impl FnMut(&&str) -> bool) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim())
        // filter typechanges
        .filter(filter)
        .filter_map(|e| e.rsplit(' ').next())
        .map(|e| e.to_string())
        .collect()
}

#[instrument]
pub fn git_in_dir(dir: &Utf8Path, args: &[&str]) -> anyhow::Result<String> {
    let args: Vec<&str> = args.iter().map(|s| s.trim()).collect();
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(&args)
        .output()
        .with_context(|| {
            format!("error while running git in directory `{dir:?}` with args `{args:?}`")
        })?;
    trace!("git {:?}: output = {:?}", args, output);
    let stdout = cmd::string_from_bytes(output.stdout)?;
    if output.status.success() {
        Ok(stdout)
    } else {
        let mut error =
            format!("error while running git in directory `{dir:?}` with args `{args:?}");
        let stderr = cmd::string_from_bytes(output.stderr)?;
        if !stdout.is_empty() || !stderr.is_empty() {
            error.push(':');
        }
        if !stdout.is_empty() {
            error.push_str("\n- stdout: ");
            error.push_str(&stdout);
        }
        if !stderr.is_empty() {
            error.push_str("\n- stderr: ");
            error.push_str(&stderr);
        }
        Err(anyhow!(error))
    }
}

/// Get the name of the current branch.
fn get_current_branch(directory: impl AsRef<Utf8Path>) -> anyhow::Result<String> {
    git_in_dir(directory.as_ref(), &["rev-parse", "--abbrev-ref", "HEAD"]).map_err(|e| {
        if e.to_string().contains(
            "fatal: ambiguous argument 'HEAD': unknown revision or path not in the working tree.",
        ) {
            anyhow!("git repository does not contain any commit.")
        } else {
            e
        }
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn inexistent_previous_commit_detected() {
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file1 = repository_dir.as_ref().join("file1.txt");
        repo.checkout_previous_commit_at_paths(&[&file1])
            .unwrap_err();
    }

    #[test]
    fn previous_commit_is_retrieved() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file1 = repository_dir.as_ref().join("file1.txt");
        let file2 = repository_dir.as_ref().join("file2.txt");
        {
            fs_err::write(&file2, b"Hello, file2!-1").unwrap();
            repo.add_all_and_commit("file2-1").unwrap();
            fs_err::write(file1, b"Hello, file1!").unwrap();
            repo.add_all_and_commit("file1").unwrap();
            fs_err::write(&file2, b"Hello, file2!-2").unwrap();
            repo.add_all_and_commit("file2-2").unwrap();
        }
        repo.checkout_previous_commit_at_paths(&[&file2]).unwrap();
        assert_eq!(repo.current_commit_message().unwrap(), "file2-1");
    }

    /// A feature branch that forks *before* a release and merges *after* it must not hide
    /// the commits that landed on the main line between the release and the merge.
    /// Regression test for release-plz#2494.
    #[test]
    fn commits_at_paths_since_keeps_main_line_after_a_straddling_merge() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let pkg = repository_dir.as_ref().join("pkg");
        fs_err::create_dir_all(&pkg).unwrap();
        let lib = pkg.join("lib.rs");

        // Base commit. The feature branch forks from here, i.e. before the release.
        fs_err::write(&lib, b"0").unwrap();
        repo.add_all_and_commit("base").unwrap();
        let base = repo.current_commit_hash().unwrap();

        // Release commit on the main line, tagged: this is the last released version.
        fs_err::write(&lib, b"1").unwrap();
        repo.add_all_and_commit("release").unwrap();
        repo.git(&["tag", "v1"]).unwrap();
        let release = repo.current_commit_hash().unwrap();

        // Two commits that land on the main line after the release. These are the commits
        // the old commit-by-commit walk dropped.
        fs_err::write(&lib, b"2").unwrap();
        repo.add_all_and_commit("main-after-release-1").unwrap();
        let main_1 = repo.current_commit_hash().unwrap();
        fs_err::write(pkg.join("extra.rs"), b"x").unwrap();
        repo.add_all_and_commit("main-after-release-2").unwrap();
        let main_2 = repo.current_commit_hash().unwrap();

        // A feature branch forked from `base` (before the release) and merged after it.
        repo.git(&["checkout", "-b", "feature", &base]).unwrap();
        fs_err::write(pkg.join("feature.rs"), b"f").unwrap();
        repo.add_all_and_commit("feature").unwrap();
        let feature = repo.current_commit_hash().unwrap();
        repo.checkout(&main_2).unwrap();
        repo.git(&["merge", "--no-ff", "feature", "-m", "merge feature"])
            .unwrap();

        let commits = repo
            .commits_at_paths_since("HEAD", &[&release], &[pkg.as_ref()], u32::MAX)
            .unwrap();

        // All three commits made after the release are collected, regardless of the branch
        // they landed on.
        for expected in [&main_1, &main_2, &feature] {
            assert!(
                commits.contains(expected),
                "expected commit {expected} to be collected, got {commits:?}"
            );
        }
        // Already-released commits are excluded.
        for released in [&base, &release] {
            assert!(
                !commits.contains(released),
                "released commit {released} should not be collected, got {commits:?}"
            );
        }
    }

    /// A boundary that is not an ancestor of `HEAD` (a newer release published on another
    /// branch, or a hash missing from a shallow clone) is ignored, so the commits it shares
    /// with `HEAD` are not dropped.
    #[test]
    fn commits_at_paths_since_ignores_non_ancestor_boundaries() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file = repository_dir.as_ref().join("file.rs");

        fs_err::write(&file, b"0").unwrap();
        repo.add_all_and_commit("first").unwrap();
        let first = repo.current_commit_hash().unwrap();

        // A commit on a divergent branch: it shares `first` with the main line but is not an
        // ancestor of it. Excluding it would wrongly drop `first`.
        repo.git(&["checkout", "-b", "other"]).unwrap();
        fs_err::write(repository_dir.as_ref().join("other.rs"), b"x").unwrap();
        repo.add_all_and_commit("other").unwrap();
        let other = repo.current_commit_hash().unwrap();

        // Back on the main line, advancing HEAD past the fork point.
        repo.checkout(&first).unwrap();
        fs_err::write(&file, b"1").unwrap();
        repo.add_all_and_commit("second").unwrap();
        let second = repo.current_commit_hash().unwrap();

        let bogus = "0000000000000000000000000000000000000000";
        for boundary in [other.as_str(), bogus] {
            let commits = repo
                .commits_at_paths_since("HEAD", &[boundary], &[file.as_ref()], u32::MAX)
                .unwrap();
            assert!(
                commits.contains(&first) && commits.contains(&second),
                "boundary {boundary} should have been ignored, got {commits:?}"
            );
        }
    }

    /// The walk is bounded by the `head` argument, not by whatever commit is currently checked
    /// out. A package untouched since before the release must not dump its entire history just
    /// because the caller moved `HEAD` to an old commit (the release boundary is still an
    /// ancestor of `head`, so it applies).
    #[test]
    fn commits_at_paths_since_bounds_by_head_arg_not_checked_out_commit() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let stable = repository_dir.as_ref().join("stable.rs");
        let other = repository_dir.as_ref().join("other.rs");

        // `stable.rs` is last touched here, before the release.
        fs_err::write(&stable, b"0").unwrap();
        repo.add_all_and_commit("touch stable").unwrap();
        let stable_commit = repo.current_commit_hash().unwrap();

        // The release, then further history that does not touch `stable.rs`.
        fs_err::write(&other, b"a").unwrap();
        repo.add_all_and_commit("release").unwrap();
        let release = repo.current_commit_hash().unwrap();
        fs_err::write(&other, b"b").unwrap();
        repo.add_all_and_commit("after release").unwrap();
        let head = repo.current_commit_hash().unwrap();

        // Simulate the caller checking out the last commit that touched `stable.rs` (older than
        // the release) before computing the diff.
        repo.checkout(&stable_commit).unwrap();

        let commits = repo
            .commits_at_paths_since(&head, &[&release], &[stable.as_ref()], u32::MAX)
            .unwrap();
        assert!(
            commits.is_empty(),
            "package unchanged since before the release should yield no commits, got {commits:?}"
        );
    }

    #[test]
    fn current_commit_is_retrieved() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file1 = repository_dir.as_ref().join("file1.txt");

        let commit_message = r"feat: my feature

        message

        footer: small note";

        {
            fs_err::write(file1, b"Hello, file1!").unwrap();
            repo.add_all_and_commit(commit_message).unwrap();
        }
        assert_eq!(repo.current_commit_message().unwrap(), commit_message);
    }

    #[test]
    fn clean_project_is_recognized() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        repo.is_clean().unwrap();
    }

    #[test]
    fn dirty_project_is_recognized() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file1 = repository_dir.as_ref().join("file1.txt");
        fs_err::write(file1, b"Hello, file1!").unwrap();
        assert!(repo.is_clean().is_err());
    }

    #[test]
    fn changes_files_except_typechanges_are_detected() {
        let git_status_output = r"T CHANGELOG.md
M README.md
A  crates
D  crates/git_cmd/CHANGELOG.md
";
        let changed_files = changed_files(git_status_output, |line| !line.starts_with("T "));
        // `CHANGELOG.md` is ignored because it's a typechange
        let expected_changed_files = vec!["README.md", "crates", "crates/git_cmd/CHANGELOG.md"];
        assert_eq!(changed_files, expected_changed_files);
    }

    #[test]
    fn existing_tag_is_recognized() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file1 = repository_dir.as_ref().join("file1.txt");
        {
            fs_err::write(file1, b"Hello, file1!").unwrap();
            repo.add_all_and_commit("file1").unwrap();
        }
        let version = "v1.0.0";
        repo.tag(version, "test").unwrap();
        assert!(repo.tag_exists(version).unwrap());
    }

    #[test]
    fn non_existing_tag_is_recognized() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let file1 = repository_dir.as_ref().join("file1.txt");
        {
            fs_err::write(file1, b"Hello, file1!").unwrap();
            repo.add_all_and_commit("file1").unwrap();
        }
        repo.tag("v1.0.0", "test").unwrap();
        assert!(!repo.tag_exists("v2.0.0").unwrap());
    }

    #[test]
    fn tags_are_retrieved() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        repo.tag("v1.0.0", "test").unwrap();
        let file1 = repository_dir.as_ref().join("file1.txt");
        {
            fs_err::write(file1, b"Hello, file1!").unwrap();
            repo.add_all_and_commit("file1").unwrap();
        }
        repo.tag("v1.0.1", "test2").unwrap();
        let tags = repo.get_all_tags();
        assert_eq!(tags, vec!["v1.0.0", "v1.0.1"]);
    }

    #[test]
    fn is_branch_of_commit_detected_correctly() {
        test_logs::init();
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let commit_hash = repo.current_commit_hash().unwrap();
        let branches = repo.get_branches_of_commit(&commit_hash).unwrap();
        assert_eq!(branches, vec![repo.original_branch()]);
    }
}
