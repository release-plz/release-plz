use git_cmd::Repo;
use k_releaser_core::RepoUrl;

use crate::config::Config;

use super::manifest_command::ManifestCommand;

/// Command that acts on a repo.
pub trait RepoCommand: ManifestCommand {
    fn repo_url(&self) -> Option<&str>;

    fn get_repo_url(&self, config: &Config) -> anyhow::Result<RepoUrl> {
        match &self.user_repo_url(config) {
            Some(url) => RepoUrl::new(url),
            None => {
                let manifest_path = self.manifest_path();
                let project_dir = k_releaser_core::manifest_dir(&manifest_path)?;
                let repo = Repo::new(project_dir)?;
                RepoUrl::from_repo(&repo)
            }
        }
    }

    /// Repo url specified by user
    fn user_repo_url<'a>(&'a self, config: &'a Config) -> Option<&'a str> {
        self.repo_url()
            .or_else(|| config.workspace.repo_url.as_ref().map(|u| u.as_str()))
    }
}
