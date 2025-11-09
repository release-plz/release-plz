use release_plz_core::ReleasePrRequest;

use crate::config::Config;

use super::{OutputType, update::Update};

#[derive(clap::Parser, Debug)]
pub struct ReleasePr {
    #[command(flatten)]
    pub update: Update,
    /// Output format. If specified, prints the branch, URL and number of
    /// the release PR, if any.
    #[arg(short, long, value_enum)]
    pub output: Option<OutputType>,
}

impl ReleasePr {
    // combines the user provided configuration and package / workspace metadata into an internal
    // representation that can be passed around internally.
    pub fn release_pr_req(
        &self,
        config: &Config,
        cargo_metadata: cargo_metadata::Metadata,
    ) -> anyhow::Result<ReleasePrRequest> {
        let pr_branch_prefix = config.workspace.pr_branch_prefix.clone();
        let pr_name = config.workspace.pr_name.clone();
        let pr_body = config.workspace.pr_body.clone();
        let pr_labels = config.workspace.pr_labels.clone();
        let pr_draft = config.workspace.pr_draft;

        let update_request = self.update.update_request(config, cargo_metadata)?;
        let request = ReleasePrRequest::new(update_request)
            .mark_as_draft(pr_draft)
            .with_labels(pr_labels)
            .with_branch_prefix(pr_branch_prefix)
            .with_pr_name_template(pr_name)
            .with_pr_body_template(pr_body);
        Ok(request)
    }
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
