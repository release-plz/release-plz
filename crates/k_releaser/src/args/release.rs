use std::path::{Path, PathBuf};

use clap::{
    ValueEnum,
    builder::{NonEmptyStringValueParser, PathBufValueParser},
};
use k_releaser_core::{GitForge, GitHub, GitLab, Gitea, ReleaseRequest};
use secrecy::SecretString;

use crate::config::Config;

use super::{
    OutputType, config_path::ConfigPath, manifest_command::ManifestCommand,
    repo_command::RepoCommand,
};

#[derive(clap::Parser, Debug)]
pub struct Release {
    /// Path to the Cargo.toml of the project you want to release.
    /// If not provided, k-releaser will use the Cargo.toml of the current directory.
    /// Both Cargo workspaces and single packages are supported.
    #[arg(long, value_parser = PathBufValueParser::new(), alias = "project-manifest")]
    manifest_path: Option<PathBuf>,

    /// Perform all checks without creating git tags/releases.
    #[arg(long)]
    pub dry_run: bool,

    /// GitHub/Gitea/GitLab repository url where your project is hosted.
    /// It is used to create the git release.
    /// It defaults to the url of the default remote.
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    pub repo_url: Option<String>,

    /// Git token used to publish the GitHub/Gitea/GitLab release.
    #[arg(long, value_parser = NonEmptyStringValueParser::new(), env, hide_env_values=true)]
    pub git_token: Option<String>,

    /// Kind of git forge
    #[arg(long, visible_alias = "backend", value_enum, default_value_t = ReleaseGitForgeKind::Github)]
    forge: ReleaseGitForgeKind,

    /// Path to the k-releaser config file.
    #[command(flatten)]
    pub config: ConfigPath,

    /// Output format. If specified, prints the version and the tag of the
    /// released packages.
    #[arg(short, long, value_enum)]
    pub output: Option<OutputType>,
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseGitForgeKind {
    #[value(name = "github")]
    Github,
    #[value(name = "gitea")]
    Gitea,
    #[value(name = "gitlab")]
    Gitlab,
}

impl Release {
    pub fn release_request(
        self,
        config: &Config,
        metadata: cargo_metadata::Metadata,
    ) -> anyhow::Result<ReleaseRequest> {
        let git_release = if let Some(git_token) = &self.git_token {
            let git_token = SecretString::from(git_token.clone());
            let repo_url = self.get_repo_url(config)?;
            let release = k_releaser_core::GitRelease {
                forge: match self.forge {
                    ReleaseGitForgeKind::Gitea => GitForge::Gitea(Gitea::new(repo_url, git_token)?),
                    ReleaseGitForgeKind::Github => {
                        GitForge::Github(GitHub::new(repo_url.owner, repo_url.name, git_token))
                    }
                    ReleaseGitForgeKind::Gitlab => {
                        GitForge::Gitlab(GitLab::new(repo_url, git_token)?)
                    }
                },
            };
            Some(release)
        } else {
            None
        };
        let mut req = ReleaseRequest::new(metadata).with_dry_run(self.dry_run);

        if let Some(repo_url) = self.repo_url {
            req = req.with_repo_url(repo_url);
        }
        if let Some(git_release) = git_release {
            req = req.with_git_release(git_release);
        }
        if let Some(release_always) = config.workspace.release_always {
            req = req.with_release_always(release_always);
        }

        req = config.fill_release_config(false, false, req);

        req = req.with_branch_prefix(config.workspace.pr_branch_prefix.clone());

        Ok(req)
    }
}

impl RepoCommand for Release {
    fn repo_url(&self) -> Option<&str> {
        self.repo_url.as_deref()
    }
}

impl ManifestCommand for Release {
    fn optional_manifest(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use fake_package::metadata::fake_metadata;

    use super::*;

    fn default_args() -> Release {
        Release {
            manifest_path: None,
            dry_run: false,
            repo_url: None,
            git_token: None,
            forge: ReleaseGitForgeKind::Github,
            config: ConfigPath::default(),
            output: None,
        }
    }

    #[test]
    fn default_config_is_converted_to_default_release_request() {
        let release_args = default_args();
        let config: Config = toml::from_str("").unwrap();
        let request = release_args
            .release_request(&config, fake_metadata())
            .unwrap();
        let pkg_config = request.get_package_config("aaa");
        let expected = k_releaser_core::ReleaseConfig::default();
        assert_eq!(pkg_config, expected);
        assert!(pkg_config.git_release().is_enabled());
    }
}
