use std::collections::HashMap;

use anyhow::Context as _;
use git_cliff_core::{config::ChangelogConfig, contributor::RemoteContributor};
use git_cmd::Repo;

use crate::{GitClient, NO_COMMIT_ID, diff::Commit};

#[derive(Debug)]
pub struct RequiredInfo {
    author: SignatureFields,
    committer: SignatureFields,
    remote_username: bool,
    remote_pr_number: bool,
}

#[derive(Debug)]
struct SignatureFields {
    name: bool,
    email: bool,
}

impl SignatureFields {
    fn is_required(&self) -> bool {
        self.name || self.email
    }
}

impl RequiredInfo {
    fn is_local_required(&self) -> bool {
        self.author.is_required() || self.committer.is_required()
    }

    pub(crate) fn is_remote_required(&self) -> bool {
        self.remote_username || self.remote_pr_number
    }
}

pub async fn fill_commit<'a>(
    commit: &'a mut Commit,
    required_info: &RequiredInfo,
    repository: &Repo,
    all_commits: &mut HashMap<String, &'a Commit>,
    git_client: Option<&GitClient>,
) -> anyhow::Result<()> {
    if !required_info.is_local_required() && !required_info.is_remote_required() {
        return Ok(());
    }
    if let Some(existing_commit) = all_commits.get(&commit.id) {
        commit.author = existing_commit.author.clone();
        commit.committer = existing_commit.committer.clone();
        commit.remote = existing_commit.remote.clone();
    } else {
        if required_info.is_local_required() {
            let signatures = repository.get_commit_signatures(&commit.id)?;
            if required_info.author.name {
                commit.author.name = Some(signatures.author_name);
            }
            if required_info.author.email {
                commit.author.email = Some(signatures.author_email);
            }
            if required_info.committer.name {
                commit.committer.name = Some(signatures.committer_name);
            }
            if required_info.committer.email {
                commit.committer.email = Some(signatures.committer_email);
            }
        }
        if required_info.is_remote_required() {
            let git_client = git_client
                .context("The changelog template requires information from the remote, but git token wasn't provided")?;
            let username = if required_info.remote_username && commit.id != NO_COMMIT_ID {
                git_client.get_remote_commit(&commit.id).await?.username
            } else {
                None
            };
            let pr_number = if required_info.remote_pr_number && commit.id != NO_COMMIT_ID {
                let associated_prs = git_client.associated_prs(&commit.id).await?;
                associated_prs.first().map(|pr| pr.number)
            } else {
                None
            };

            commit.remote = RemoteContributor {
                username,
                pr_number: pr_number.and_then(|n| i64::try_from(n).ok()),
                ..RemoteContributor::default()
            };
        }
        all_commits.insert(commit.id.clone(), commit);
    }
    Ok(())
}

pub fn get_required_info(changelog_config: &ChangelogConfig) -> RequiredInfo {
    let body = &changelog_config.body;
    RequiredInfo {
        author: SignatureFields {
            name: body.contains("author.name"),
            email: body.contains("author.email"),
        },
        committer: SignatureFields {
            name: body.contains("committer.name"),
            email: body.contains("committer.email"),
        },
        remote_username: body.contains("remote.username"),
        remote_pr_number: body.contains("remote.pr_number"),
    }
}
