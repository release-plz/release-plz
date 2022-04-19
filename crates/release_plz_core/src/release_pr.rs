use std::path::PathBuf;

use cargo_metadata::Package;
use chrono::SecondsFormat;
use git_cmd::Repo;

use anyhow::{anyhow, Context};
use tracing::instrument;

use crate::{
    copy_to_temp_dir,
    github_client::{GitHub, GitHubClient, Pr},
    update, UpdateRequest, UpdateResult, CARGO_TOML,
};

#[derive(Debug)]
pub struct ReleasePrRequest {
    pub github: GitHub,
    pub update_request: UpdateRequest,
}

/// Open a pull request with the next packages versions of a local rust project
#[instrument]
pub async fn release_pr(input: &ReleasePrRequest) -> anyhow::Result<()> {
    let manifest_dir = input
        .update_request
        .local_manifest()
        .parent()
        .ok_or_else(|| anyhow!("wrong local manifest path"))?;
    let tmp_project_root = copy_to_temp_dir(manifest_dir)?;
    let manifest_dir_name = manifest_dir
        .iter()
        .last()
        .ok_or_else(|| anyhow!("wrong local manifest path"))?;
    let manifest_dir_name = PathBuf::from(manifest_dir_name);
    let new_manifest_dir = tmp_project_root.as_ref().join(manifest_dir_name);
    let local_manifest = new_manifest_dir.join(CARGO_TOML);
    let new_update_request = input
        .update_request
        .clone()
        .set_local_manifest(local_manifest)
        .context("can't find temporary project")?;
    let (packages_to_update, _repository) = update(&new_update_request)?;
    let gh_client = GitHubClient::new(&input.github)?;
    gh_client.close_other_prs()?;
    if !packages_to_update.is_empty() {
        let repo = Repo::new(new_manifest_dir)?;
        let pr = Pr::from(packages_to_update.as_ref());
        create_release_branch(&repo, &pr.branch)?;
        gh_client.open_pr(&pr).await?;
    }

    Ok(())
}

impl From<&[(Package, UpdateResult)]> for Pr {
    fn from(packages_to_update: &[(Package, UpdateResult)]) -> Self {
        Self {
            branch: release_branch(),
            title: pr_title(packages_to_update),
        }
    }
}

fn release_branch() -> String {
    let now = chrono::offset::Utc::now();
    // Convert to a string of format "2018-01-26T18:30:09Z".
    let now = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    // ':' is not a valid character for a branch name.
    let now = now.replace(':', "-");
    format!("release-plz/{now}")
}

fn pr_title(packages_to_update: &[(Package, UpdateResult)]) -> String {
    if packages_to_update.len() == 1 {
        let (package, update) = &packages_to_update[0];
        format!("chore({}): release v{}", package.name, update.version)
    } else {
        "chore: release".to_string()
    }
}

fn create_release_branch(repository: &Repo, release_branch: &str) -> anyhow::Result<()> {
    repository.checkout_new_branch(release_branch)?;
    repository.add_all_and_commit("chore: release")?;
    repository.push(release_branch)?;
    Ok(())
}
