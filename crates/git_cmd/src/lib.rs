//! Run git as shell shell and parse its stdout.

mod cmd;
#[cfg(feature = "test_fixture")]
pub mod test_fixture;

use std::{collections::HashSet, path::Path, process::Command};

use anyhow::{Context, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use secrecy::{ExposeSecret, SecretString};
use tracing::{Span, debug, instrument, trace, warn};

/// Repository
#[derive(Debug)]
pub struct Repo {
    /// Directory where you want to run git operations
    directory: Utf8PathBuf,
    /// Branch name, or commit hash if HEAD was detached, before running any git operation
    original_branch: String,
    /// Remote name before running any git operation
    original_remote: String,
    http_extra_header: Option<HttpExtraHeader>,
}

#[derive(Debug)]
struct HttpExtraHeader {
    url: SecretString,
    header: SecretString,
}

impl HttpExtraHeader {
    fn git_config_parameters(&self, mut parameters: String) -> String {
        let key = format!("http.{}.extraHeader", self.url.expose_secret()).replace('\'', "'\\''");
        // Git passes `-c` options to its subprocesses as shell-quoted parameters.
        // Append our reset and header after inherited options to retain their precedence.
        // Keep the key private too: the URL's username may contain a token.
        for value in ["", self.header.expose_secret()] {
            let value = value.replace('\'', "'\\''");
            if !parameters.is_empty() {
                parameters.push(' ');
            }
            parameters.push_str(&format!("'{key}'='{value}'"));
        }
        parameters
    }
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
            http_extra_header: None,
        })
    }

    pub fn directory(&self) -> &Utf8Path {
        &self.directory
    }

    /// Send an HTTP header to this URL when fetching, pulling or pushing.
    /// The header is passed through the command environment, never stored in git config.
    pub fn with_http_extra_header(mut self, url: String, header: SecretString) -> Self {
        self.http_extra_header = Some(HttpExtraHeader {
            url: url.into(),
            header,
        });
        self
    }

    fn git_remote(&self, args: &[&str]) -> anyhow::Result<String> {
        let Some(auth) = &self.http_extra_header else {
            return self.git(args);
        };
        let inherited = std::env::var_os("GIT_CONFIG_PARAMETERS")
            .unwrap_or_default()
            .into_string()
            .map_err(|_invalid_parameters| anyhow!("GIT_CONFIG_PARAMETERS is not UTF-8"))?;
        let parameters = auth.git_config_parameters(inherited);
        git_in_dir_with_env(
            &self.directory,
            args,
            &[("GIT_CONFIG_PARAMETERS", &parameters)],
        )
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
                } else if err.contains("fatal: HEAD does not point to a branch") {
                    // Save the commit, rather than the symbolic HEAD, so checkout_head
                    // can restore it after checking out older commits.
                    let commit = git_in_dir(directory.as_ref(), &["rev-parse", "HEAD"])?;
                    Ok(("origin".to_string(), commit))
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
            "the working directory of this project has uncommitted changes.\n\
             Please commit or stash these changes:\n{changes:?}"
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
        self.git_remote(&["push", &self.original_remote, obj])?;
        Ok(())
    }

    pub fn fetch(&self, obj: &str) -> anyhow::Result<()> {
        self.git_remote(&["fetch", &self.original_remote, obj])
            .with_context(|| format!("failed to fetch {obj}"))?;
        Ok(())
    }

    pub fn pull(&self) -> anyhow::Result<()> {
        self.git_remote(&["pull"])?;
        Ok(())
    }

    pub fn force_push(&self, obj: &str) -> anyhow::Result<()> {
        // `--force-with-lease` is safer than `--force` because it will not overwrite
        // changes on the remote that you do not have locally.
        // In other words, it will only push if no one else has pushed changes to the remote
        // branch since you last pulled. If someone else has pushed changes, the command will fail,
        // preventing you from accidentally overwriting someone else's work.
        self.git_remote(&["push", &self.original_remote, obj, "--force-with-lease"])
            .with_context(|| format!("failed to force-push {obj}"))?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn checkout_head(&self) -> anyhow::Result<()> {
        self.checkout(&self.original_branch)?;
        Ok(())
    }

    /// Branch name, or commit hash if HEAD was detached, before running any git operation.
    /// I.e. when the [`Repo`] was created.
    pub fn original_branch(&self) -> &str {
        &self.original_branch
    }

    /// Whether HEAD points directly to a commit rather than to a branch.
    pub fn is_head_detached(&self) -> anyhow::Result<bool> {
        // `rev-parse --abbrev-ref HEAD` prints `HEAD` when HEAD is detached.
        Ok(get_current_branch(&self.directory)? == "HEAD")
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

    /// Commits reachable from `head` that touch `paths`, ordered with `--date-order`.
    ///
    /// `exclude` commits and their ancestors are dropped. An `exclude` entry that
    /// doesn't exist in this repository is ignored, so a commit hash recorded by a
    /// release that happened in another repository, or missing from a shallow clone,
    /// is not fatal.
    ///
    /// Commits are ordered by date rather than topologically: `--topo-order` emits
    /// whole lineages contiguously, so combining it with `max_commits` would keep the
    /// oldest commits of one branch instead of the newest commits overall.
    pub fn commits_at_paths(
        &self,
        head: &str,
        exclude: &[&str],
        paths: &[impl AsRef<Utf8Path>],
        max_commits: Option<u32>,
    ) -> anyhow::Result<Vec<String>> {
        let limit = max_commits.map(|n| format!("--max-count={n}"));
        let mut args = vec!["--date-order", head];
        args.extend(limit.as_deref());
        self.rev_list(&args, exclude, paths)
    }

    /// Commits reachable from `commit` that touch `paths`.
    ///
    /// As in [`Repo::commits_at_paths`], `exclude` commits and their ancestors are
    /// dropped, and missing exclusions are ignored.
    ///
    /// Unlike [`Repo::commits_at_paths`], this doesn't simplify history: every
    /// parent of a merge is followed, so the result is a superset of the commits any
    /// simplified walk can reach through `commit` with the same exclusions.
    pub fn ancestors_at_paths(
        &self,
        commit: &str,
        exclude: &[&str],
        paths: &[impl AsRef<Utf8Path>],
    ) -> anyhow::Result<Vec<String>> {
        self.rev_list(&["--full-history", commit], exclude, paths)
    }

    /// Run `git rev-list` with `args`, restricted to the commits touching `paths`.
    fn rev_list(
        &self,
        args: &[&str],
        exclude: &[&str],
        paths: &[impl AsRef<Utf8Path>],
    ) -> anyhow::Result<Vec<String>> {
        let exclusions: Vec<String> = exclude
            .iter()
            .filter(|commit| self.commit_exists(commit))
            .map(|commit| format!("^{commit}"))
            .collect();
        let mut rev_list = vec!["rev-list"];
        rev_list.extend(args);
        rev_list.extend(exclusions.iter().map(String::as_str));
        rev_list.push("--");
        rev_list.extend(paths.iter().map(|p| p.as_ref().as_str()));
        let output = self.git(&rev_list)?;
        Ok(output.lines().map(str::to_owned).collect())
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

    /// Whether `object` resolves to a commit in this repository.
    fn commit_exists(&self, object: &str) -> bool {
        self.git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{object}^{{commit}}"),
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
    git_in_dir_with_env(dir, args, &[])
}

/// Like [`git_in_dir`], but with `envs` added to the environment of the git process.
pub(crate) fn git_in_dir_with_env(
    dir: &Utf8Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> anyhow::Result<String> {
    let args: Vec<&str> = args.iter().map(|s| s.trim()).collect();
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(&args)
        .envs(envs.iter().copied())
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
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    use super::*;

    #[tokio::test]
    async fn http_header_is_scoped_to_remote_commands_and_repository_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let url = format!("{}/owner/repo.git", server.uri());
        repo.git(&["remote", "add", "origin", &url]).unwrap();
        // actions/checkout can leave a host-wide authentication header behind.
        let checkout_key = format!("http.{}/.extraHeader", server.uri());
        repo.git(&["config", &checkout_key, "Authorization: Basic checkout"])
            .unwrap();
        let header = "Authorization: Basic secret";
        let repo = repo.with_http_extra_header(url, SecretString::from(header));
        assert!(!format!("{repo:?}").contains(header));
        for error in [
            repo.fetch("main").unwrap_err(),
            repo.push("HEAD:main").unwrap_err(),
            repo.force_push("HEAD:main").unwrap_err(),
            repo.pull().unwrap_err(),
        ] {
            assert!(!format!("{error:?}").contains(header));
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 4);
        for request in requests {
            let authorization: Vec<_> = request.headers.get_all("authorization").iter().collect();
            assert_eq!(authorization, ["Basic secret"]);
        }

        // The provided header is never persisted, and local commands don't receive it.
        let config = repo.git(&["config", "--list"]).unwrap();
        assert!(!config.contains("secret"));
        assert!(config.contains("Authorization: Basic checkout"));
        repo.git(&["config", "--unset-all", &checkout_key]).unwrap();

        // Another repository on the same host must not receive the secret.
        repo.git(&[
            "remote",
            "set-url",
            "origin",
            &format!("{}/owner/other.git", server.uri()),
        ])
        .unwrap();
        repo.fetch("main").unwrap_err();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 5);
        assert!(!requests[4].headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn http_header_overrides_username_scoped_checkout_credentials() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let url = format!("{}/owner/repo.git", server.uri()).replacen(
            "http://",
            "http://checkout'token=value@",
            1,
        );
        repo.git(&["remote", "add", "origin", &url]).unwrap();
        let key = format!("http.{url}.extraHeader");
        repo.git(&["config", &key, "Authorization: Basic checkout"])
            .unwrap();
        let header = "Authorization: Basic supplied'token";
        let repo = repo.with_http_extra_header(url, header.into());
        let debug = format!("{repo:?}");
        assert!(!debug.contains("checkout'token"));
        assert!(!debug.contains("supplied'token"));
        let error = repo.fetch("main").unwrap_err();
        assert!(!format!("{error:?}").contains("supplied'token"));

        // Preserve inherited entries and override even an inherited header for this exact URL.
        // Pass the environment directly to avoid changing process-wide state in parallel tests.
        let inherited = format!(
            "'http.userAgent=inherited-agent' '{}'='Authorization: Basic inherited'",
            key.replace('\'', "'\\''")
        );
        let parameters = repo
            .http_extra_header
            .as_ref()
            .unwrap()
            .git_config_parameters(inherited);
        git_in_dir_with_env(
            repo.directory(),
            &["fetch", "origin", "main"],
            &[
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "http.extraHeader"),
                ("GIT_CONFIG_VALUE_0", "Authorization: Basic inherited-count"),
                ("GIT_CONFIG_PARAMETERS", &parameters),
            ],
        )
        .unwrap_err();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for request in &requests {
            let authorization: Vec<_> = request.headers.get_all("authorization").iter().collect();
            assert_eq!(authorization, ["Basic supplied'token"]);
        }
        assert_eq!(requests[1].headers["user-agent"], "inherited-agent");
        let config = repo.git(&["config", "--local", "--list"]).unwrap();
        assert!(!config.contains("supplied'token"));
        assert!(config.contains("Authorization: Basic checkout"));
    }

    /// `git rev-list --topo-order` emits whole lineages contiguously, so combining
    /// it with `--max-count` returns the oldest commits of one branch instead of the
    /// newest commits overall. Every mainline commit would be dropped from a first
    /// release limited by `max_analyze_commits`.
    #[test]
    fn commit_limit_keeps_the_newest_commit_of_every_branch() {
        test_logs::init();
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let path = Utf8Path::new("pkg");
        fs_err::create_dir(directory.path().join(path)).unwrap();
        let main_branch = repo.original_branch().to_string();

        // Interleave the commit dates of the two branches, so taking the newest
        // three commits must take from both sides.
        commit_file_at(&repo, path, "base", "2024-01-01T00:00:00 +0000");
        repo.git(&["branch", "other"]).unwrap();
        for (name, date) in [
            ("a1", "2024-01-01T00:00:01 +0000"),
            ("a2", "2024-01-01T00:00:03 +0000"),
            ("a3", "2024-01-01T00:00:05 +0000"),
        ] {
            commit_file_at(&repo, path, name, date);
        }
        let a3 = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", "other"]).unwrap();
        for (name, date) in [
            ("b1", "2024-01-01T00:00:02 +0000"),
            ("b2", "2024-01-01T00:00:04 +0000"),
            ("b3", "2024-01-01T00:00:06 +0000"),
        ] {
            commit_file_at(&repo, path, name, date);
        }
        let b3 = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", &main_branch]).unwrap();
        commit_merge_at(&repo, "other", "2024-01-01T00:00:07 +0000");
        let merge = repo.current_commit_hash().unwrap();

        assert_eq!(
            repo.commits_at_paths("HEAD", &[], &[path], Some(3))
                .unwrap(),
            [merge, b3, a3]
        );
    }

    fn commit_file_at(repo: &Repo, directory: &Utf8Path, name: &str, date: &str) {
        let file = repo.directory().join(directory).join(name);
        fs_err::write(file, name).unwrap();
        repo.git(&["add", "."]).unwrap();
        repo.git_at(&["commit", "-m", name], date).unwrap();
    }

    fn commit_merge_at(repo: &Repo, branch: &str, date: &str) {
        let message = format!("merge {branch}");
        repo.git_at(&["merge", "--no-ff", "-m", &message, branch], date)
            .unwrap();
    }

    /// [`Repo::ancestors_at_paths`] exists to not simplify history: a "keep mine"
    /// merge is TREESAME to its first parent, so git drops the branch the merge
    /// discarded from every simplified walk through it, although those commits are
    /// real ancestors of the merge.
    #[test]
    fn full_history_ancestors_keep_the_parent_a_simplified_walk_drops() {
        test_logs::init();
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let path = Utf8Path::new("pkg");
        fs_err::create_dir(directory.path().join(path)).unwrap();
        let main_branch = repo.original_branch().to_string();
        commit_file_at(&repo, path, "base", "2024-01-01T00:00:00 +0000");
        repo.git(&["checkout", "-b", "feature"]).unwrap();
        commit_file_at(&repo, path, "discarded", "2024-01-01T00:00:01 +0000");
        let discarded = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", &main_branch]).unwrap();
        commit_file_at(&repo, path, "mine", "2024-01-01T00:00:02 +0000");
        repo.git_at(
            &["merge", "-s", "ours", "-m", "merge feature", "feature"],
            "2024-01-01T00:00:03 +0000",
        )
        .unwrap();

        assert!(
            repo.is_ancestor(&discarded, "HEAD"),
            "the discarded commit must be a real ancestor of the merge"
        );
        assert!(
            repo.ancestors_at_paths("HEAD", &[], &[path])
                .unwrap()
                .contains(&discarded)
        );
        assert!(
            !repo
                .commits_at_paths("HEAD", &[], &[path], None)
                .unwrap()
                .contains(&discarded),
            "the simplified walk is supposed to miss it: that's why the two differ"
        );
    }

    #[test]
    fn full_history_ancestors_exclude_each_release_boundary() {
        test_logs::init();
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let path = Utf8Path::new("pkg");
        fs_err::create_dir(directory.path().join(path)).unwrap();
        let main_branch = repo.original_branch().to_string();
        commit_file_at(&repo, path, "base", "2024-01-01T00:00:00 +0000");
        repo.git(&["branch", "feature"]).unwrap();

        // The tag and published SHA can be on different branches. Both release
        // boundaries and their shared history must be excluded from the walk.
        commit_file_at(&repo, path, "tagged", "2024-01-01T00:00:01 +0000");
        let tagged = repo.current_commit_hash().unwrap();
        commit_file_at(&repo, path, "mine", "2024-01-01T00:00:02 +0000");
        let mine = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", "feature"]).unwrap();
        commit_file_at(&repo, path, "published", "2024-01-01T00:00:03 +0000");
        let published = repo.current_commit_hash().unwrap();
        commit_file_at(&repo, path, "discarded", "2024-01-01T00:00:04 +0000");
        let discarded = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", &main_branch]).unwrap();
        repo.git_at(
            &["merge", "-s", "ours", "-m", "merge feature", "feature"],
            "2024-01-01T00:00:05 +0000",
        )
        .unwrap();
        let merge = repo.current_commit_hash().unwrap();

        let missing = "0000000000000000000000000000000000000000";
        assert_eq!(
            repo.ancestors_at_paths("HEAD", &[missing], &[path])
                .unwrap(),
            repo.ancestors_at_paths("HEAD", &[], &[path]).unwrap(),
            "a missing release boundary must leave history available"
        );
        let ancestors: HashSet<_> = repo
            .ancestors_at_paths("HEAD", &[&tagged, &published, missing], &[path])
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(
            ancestors,
            HashSet::from([merge, mine, discarded]),
            "only unreleased ancestors remain, including the discarded merge parent"
        );
    }

    #[test]
    fn detached_head_is_restored_after_checking_out_history() {
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        let previous_commit = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", "--detach"]).unwrap();
        repo.git(&["commit", "--allow-empty", "-m", "detached commit"])
            .unwrap();
        let original_commit = repo.current_commit_hash().unwrap();
        repo.git(&[
            "remote",
            "add",
            "origin",
            "https://github.com/test/project.git",
        ])
        .unwrap();

        let repo = Repo::new(repo.directory()).unwrap();
        assert_eq!(repo.original_branch(), original_commit);
        assert_eq!(repo.original_remote(), "origin");
        assert_eq!(
            repo.original_remote_url().unwrap(),
            "https://github.com/test/project.git"
        );

        repo.checkout(&previous_commit).unwrap();
        repo.checkout_head().unwrap();
        assert_eq!(repo.current_commit_hash().unwrap(), original_commit);
        assert!(repo.is_head_detached().unwrap());
    }

    #[test]
    fn configured_upstream_is_used() {
        let repository_dir = tempdir().unwrap();
        let repo = Repo::init(&repository_dir);
        repo.git(&[
            "remote",
            "add",
            "upstream",
            "https://github.com/test/project.git",
        ])
        .unwrap();
        repo.git(&["update-ref", "refs/remotes/upstream/release/stable", "HEAD"])
            .unwrap();
        repo.git(&["branch", "--set-upstream-to=upstream/release/stable"])
            .unwrap();

        let repo = Repo::new(repo.directory()).unwrap();
        assert_eq!(repo.original_remote(), "upstream");
        assert_eq!(repo.original_branch(), "release/stable");
    }

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

    #[test]
    fn commit_range_ignores_missing_but_not_unreachable_boundaries() {
        test_logs::init();
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let path = Utf8Path::new("file.rs");
        fs_err::write(directory.path().join(path), "shared").unwrap();
        repo.add_all_and_commit("shared change").unwrap();
        let shared = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", "-b", "release"]).unwrap();
        fs_err::write(directory.path().join(path), "other branch").unwrap();
        repo.add_all_and_commit("release on another branch")
            .unwrap();
        let release = repo.current_commit_hash().unwrap();
        repo.checkout_head().unwrap();
        fs_err::write(directory.path().join(path), "local change").unwrap();
        repo.add_all_and_commit("local change").unwrap();
        let local = repo.current_commit_hash().unwrap();

        // A boundary that doesn't exist locally can't exclude anything.
        assert_eq!(
            repo.commits_at_paths(
                "HEAD",
                &["0000000000000000000000000000000000000000"],
                &[path],
                None,
            )
            .unwrap(),
            [local.clone(), shared.clone()]
        );

        // A boundary on a divergent branch still excludes the history it shares
        // with `head`: those changes were already released.
        assert_eq!(
            repo.commits_at_paths("HEAD", &[&release], &[path], None)
                .unwrap(),
            [local]
        );
    }

    #[test]
    fn commit_range_uses_the_given_tip_and_exclusions() {
        test_logs::init();
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let path = Utf8Path::new("file.rs");
        let mut commits = Vec::new();
        for message in ["tagged", "published", "unreleased"] {
            fs_err::write(directory.path().join(path), message).unwrap();
            repo.add_all_and_commit(message).unwrap();
            commits.push(repo.current_commit_hash().unwrap());
        }
        repo.checkout(&commits[0]).unwrap();
        assert_eq!(
            repo.commits_at_paths(&commits[2], &[&commits[0], &commits[1]], &[path], None)
                .unwrap(),
            [commits[2].clone()]
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
}
