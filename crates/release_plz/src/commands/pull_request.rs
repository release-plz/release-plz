use release_plz_core::{ReleasePrRequest, UpdateRequest};

use crate::{
    commands::{config::ConfigCommand, manifest::ManifestCommand, print_output},
    config::Config,
};

use super::{update::Update, OutputType};

/// Create a Pull Request representing the next release.
///
/// The Pull request will update the package version and generate a changelog entry for the new
/// version based on the commit messages. If there is a previously opened Release PR, it will be
/// closed before opening a new one.
#[derive(clap::Parser, Debug)]
pub struct PullRequest {
    #[command(flatten)]
    pub update: Update,
    /// Output format. If specified, prints the branch, URL and number of
    /// the release PR, if any.
    #[arg(short, long, value_enum)]
    pub output: Option<OutputType>,
}

impl PullRequest {
    pub async fn run(self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.update.git_token.is_some(),
            "please provide the git token with the --git-token cli argument."
        );
        let cargo_metadata = self.update.cargo_metadata()?;
        let config = self.update.config()?;
        let update_request = self.update.update_request(&config, cargo_metadata)?;
        let request = get_release_pr_req(&config, update_request)?;
        let release_pr = release_plz_core::release_pr(&request).await?;
        if let Some(output_type) = self.output {
            let prs = match release_pr {
                Some(pr) => vec![pr],
                None => vec![],
            };
            let prs_json = serde_json::json!({
                "prs": prs
            });
            print_output(output_type, prs_json);
        }
        Ok(())
    }
}

fn get_release_pr_req(
    config: &Config,
    update_request: UpdateRequest,
) -> anyhow::Result<ReleasePrRequest> {
    let pr_branch_prefix = config.workspace.pr_branch_prefix.clone();
    let pr_name = config.workspace.pr_name.clone();
    let pr_body = config.workspace.pr_body.clone();
    let pr_labels = config.workspace.pr_labels.clone();
    let pr_draft = config.workspace.pr_draft;
    let request = ReleasePrRequest::new(update_request)
        .mark_as_draft(pr_draft)
        .with_labels(pr_labels)
        .with_branch_prefix(pr_branch_prefix)
        .with_pr_name_template(pr_name)
        .with_pr_body_template(pr_body);
    Ok(request)
}

#[cfg(test)]
mod tests {
    use release_plz_core::RepoUrl;

    const GITHUB_COM: &str = "github.com";

    #[test]
    fn https_github_url_is_parsed() {
        let expected_owner = "MarcoIeni";
        let expected_repo = "release-plz";
        let url = format!("https://{GITHUB_COM}/{expected_owner}/{expected_repo}");
        let repo = RepoUrl::new(&url).unwrap();
        assert_eq!(expected_owner, repo.owner);
        assert_eq!(expected_repo, repo.name);
        assert_eq!(GITHUB_COM, repo.host);
        assert!(repo.is_on_github());
    }

    #[test]
    fn git_github_url_is_parsed() {
        let expected_owner = "MarcoIeni";
        let expected_repo = "release-plz";
        let url = format!("git@github.com:{expected_owner}/{expected_repo}.git");
        let repo = RepoUrl::new(&url).unwrap();
        assert_eq!(expected_owner, repo.owner);
        assert_eq!(expected_repo, repo.name);
        assert_eq!(GITHUB_COM, repo.host);
        assert!(repo.is_on_github());
    }

    #[test]
    fn gitea_url_is_parsed() {
        let host = "example.com";
        let expected_owner = "MarcoIeni";
        let expected_repo = "release-plz";
        let url = format!("https://{host}/{expected_owner}/{expected_repo}");
        let repo = RepoUrl::new(&url).unwrap();
        assert_eq!(expected_owner, repo.owner);
        assert_eq!(expected_repo, repo.name);
        assert_eq!(host, repo.host);
        assert_eq!("https", repo.scheme);
        assert!(!repo.is_on_github());
        assert_eq!(format!("https://{host}/api/v1/"), repo.gitea_api_url());
    }
}
