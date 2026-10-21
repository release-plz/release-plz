use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use futures::{StreamExt as _, TryStreamExt as _, stream};
use git_cliff_core::{config::ChangelogConfig, contributor::RemoteContributor};
use git_cmd::{CommitSignatures, Repo};

use crate::{GitClient, NO_COMMIT_ID, diff::Commit};

#[derive(Debug)]
pub struct RequiredInfo {
    author_name: bool,
    author_email: bool,
    committer_name: bool,
    committer_email: bool,
    remote_username: bool,
    remote_pr_number: bool,
}

impl RequiredInfo {
    fn is_local_required(&self) -> bool {
        self.author_name || self.author_email || self.committer_name || self.committer_email
    }

    pub(crate) fn is_remote_required(&self) -> bool {
        self.remote_username || self.remote_pr_number
    }

    /// Number of forge requests needed to retrieve the remote information of a commit.
    fn remote_requests_per_commit(&self) -> usize {
        usize::from(self.remote_username) + usize::from(self.remote_pr_number)
    }
}

/// Maximum number of commits whose information is requested to the forge at the same time.
///
/// Requesting one commit at a time costs a round trip per request, which adds up to several
/// seconds for a release with a few dozen commits.
/// The requests of a single commit are sequential, so this is also the maximum number
/// of in-flight requests. Ten concurrent reads is the default of GitHub's octokit
/// throttling plugin, and it's far below the 100 concurrent requests that trigger
/// GitHub's secondary rate limit.
const MAX_CONCURRENT_REMOTE_COMMITS: usize = 10;

/// Commits are fetched concurrently only if they need at most this many requests.
///
/// GitHub's secondary rate limit allows 900 REST requests per minute, and concurrent
/// requests can all complete within a minute. This leaves room for the other requests
/// of the release-plz run.
/// Bigger histories (e.g. the first release of a project with a long history)
/// are fetched one commit at a time, like before.
const MAX_CONCURRENTLY_FETCHED_REQUESTS: usize = 800;

/// Fill the commits with the information required by the changelog template.
pub async fn fill_commits(
    commits: Vec<&mut Commit>,
    required_info: &RequiredInfo,
    repository: &Repo,
    git_client: Option<&GitClient>,
) -> anyhow::Result<()> {
    if commits.is_empty()
        || (!required_info.is_local_required() && !required_info.is_remote_required())
    {
        return Ok(());
    }
    let git_client = if required_info.is_remote_required() {
        Some(git_client.context(
            "The changelog template requires information from the remote, but git token wasn't provided",
        )?)
    } else {
        None
    };
    // The same commit can be in the changelog of multiple packages,
    // so we retrieve its information only once.
    let unique_ids: HashSet<String> = commits.iter().map(|commit| commit.id.clone()).collect();

    let signatures: HashMap<&str, CommitSignatures> = if required_info.is_local_required() {
        unique_ids
            .iter()
            .map(|id| Ok((id.as_str(), repository.get_commit_signatures(id)?)))
            .collect::<anyhow::Result<_>>()?
    } else {
        HashMap::new()
    };

    let remotes = match git_client {
        Some(git_client) => remote_contributors(&unique_ids, required_info, git_client).await?,
        None => HashMap::new(),
    };

    for commit in commits {
        if let Some(signatures) = signatures.get(commit.id.as_str()) {
            if required_info.author_name {
                commit.author.name = Some(signatures.author_name.clone());
            }
            if required_info.author_email {
                commit.author.email = Some(signatures.author_email.clone());
            }
            if required_info.committer_name {
                commit.committer.name = Some(signatures.committer_name.clone());
            }
            if required_info.committer_email {
                commit.committer.email = Some(signatures.committer_email.clone());
            }
        }
        if let Some(remote) = remotes.get(commit.id.as_str()) {
            commit.remote = remote.clone();
        }
    }
    Ok(())
}

/// Retrieve the remote information of the given commits from the forge.
async fn remote_contributors<'a>(
    commit_ids: &'a HashSet<String>,
    required_info: &RequiredInfo,
    git_client: &GitClient,
) -> anyhow::Result<HashMap<&'a str, RemoteContributor>> {
    let requests = commit_ids.len() * required_info.remote_requests_per_commit();
    let concurrency = if requests <= MAX_CONCURRENTLY_FETCHED_REQUESTS {
        MAX_CONCURRENT_REMOTE_COMMITS
    } else {
        1
    };
    // Results are looked up by commit id, so the order in which requests complete
    // doesn't affect the changelog.
    stream::iter(commit_ids)
        .map(async |id| {
            let remote = remote_contributor(id, required_info, git_client).await?;
            anyhow::Ok((id.as_str(), remote))
        })
        .buffer_unordered(concurrency)
        .try_collect()
        .await
}

async fn remote_contributor(
    commit_id: &str,
    required_info: &RequiredInfo,
    git_client: &GitClient,
) -> anyhow::Result<RemoteContributor> {
    let username = if required_info.remote_username && commit_id != NO_COMMIT_ID {
        git_client.get_remote_commit(commit_id).await?.username
    } else {
        None
    };
    let pr_number = if required_info.remote_pr_number && commit_id != NO_COMMIT_ID {
        let associated_prs = git_client.associated_prs(commit_id).await?;
        associated_prs.first().map(|pr| pr.number)
    } else {
        None
    };

    Ok(RemoteContributor {
        username,
        pr_number: pr_number.and_then(|n| i64::try_from(n).ok()),
        ..RemoteContributor::default()
    })
}

pub fn get_required_info(changelog_config: &ChangelogConfig) -> RequiredInfo {
    let mut required_info = RequiredInfo {
        author_name: false,
        author_email: false,
        committer_name: false,
        committer_email: false,
        remote_username: false,
        remote_pr_number: false,
    };

    required_info.author_name = changelog_config.body.contains("author.name");
    required_info.author_email = changelog_config.body.contains("author.email");
    required_info.committer_name = changelog_config.body.contains("committer.name");
    required_info.committer_email = changelog_config.body.contains("committer.email");
    required_info.remote_username = changelog_config.body.contains("remote.username");
    required_info.remote_pr_number = changelog_config.body.contains("remote.pr_number");

    required_info
}
