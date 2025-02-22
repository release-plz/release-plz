use std::path::{Path, PathBuf};

use clap::{
    ValueEnum,
    builder::{NonEmptyStringValueParser, PathBufValueParser},
};
use release_plz_core::{GitBackend, GitHub, GitLab, Gitea, ReleaseRequest};
use secrecy::SecretString;

use crate::config::Config;

use super::{
    OutputType, config_command::ConfigCommand, manifest_command::ManifestCommand,
    repo_command::RepoCommand,
};

#[derive(clap::Parser, Debug)]
pub struct Release {
    /// Path to the Cargo.toml of the project you want to release.
    /// If not provided, release-plz will use the Cargo.toml of the current directory.
    /// Both Cargo workspaces and single packages are supported.
    #[arg(long, value_parser = PathBufValueParser::new(), alias = "project-manifest")]
    manifest_path: Option<PathBuf>,
    /// Registry where you want to publish the packages.
    /// The registry name needs to be present in the Cargo config.
    /// If unspecified, the `publish` field of the package manifest is used.
    /// If the `publish` field is empty, crates.io is used.
    #[arg(long)]
    registry: Option<String>,
    /// Token used to publish to the cargo registry.
    /// Override the `CARGO_REGISTRY_TOKEN` environment variable, or the `CARGO_REGISTRIES_<NAME>_TOKEN`
    /// environment variable, used for registry specified in the `registry` input variable.
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    token: Option<String>,
    /// Perform all checks without uploading.
    #[arg(long)]
    pub dry_run: bool,
    /// Don't verify the contents by building them.
    /// When you pass this flag, `release-plz` adds the `--no-verify` flag to `cargo publish`.
    #[arg(long)]
    pub no_verify: bool,
    /// Allow dirty working directories to be packaged.
    /// When you pass this flag, `release-plz` adds the `--allow-dirty` flag to `cargo publish`.
    #[arg(long)]
    pub allow_dirty: bool,
    /// GitHub/Gitea/Gitlab repository url where your project is hosted.
    /// It is used to create the git release.
    /// It defaults to the url of the default remote.
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    pub repo_url: Option<String>,
    /// Git token used to publish the GitHub/Gitea/GitLab release.
    #[arg(long, value_parser = NonEmptyStringValueParser::new(), env, hide_env_values=true)]
    pub git_token: Option<String>,
    /// Kind of git backend
    #[arg(long, value_enum, default_value_t = ReleaseGitBackendKind::Github)]
    backend: ReleaseGitBackendKind,
    /// Path to the release-plz config file.
    /// Default: `./release-plz.toml`.
    /// If no config file is found, the default configuration is used.
    #[arg(
        long,
        value_name = "PATH",
        value_parser = PathBufValueParser::new()
    )]
    config: Option<PathBuf>,
    /// Output format. If specified, prints the version and the tag of the
    /// released packages.
    #[arg(short, long, value_enum)]
    pub output: Option<OutputType>,
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseGitBackendKind {
    #[value(name = "github")]
    Github,
    #[value(name = "gitea")]
    Gitea,
    #[value(name = "gitlab")]
    Gitlab,
}

impl ConfigCommand for Release {
    fn config_path(&self) -> Option<&Path> {
        self.config.as_deref()
    }
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
            let release = release_plz_core::GitRelease {
                backend: match self.backend {
                    ReleaseGitBackendKind::Gitea => {
                        GitBackend::Gitea(Gitea::new(repo_url, git_token)?)
                    }
                    ReleaseGitBackendKind::Github => {
                        GitBackend::Github(GitHub::new(repo_url.owner, repo_url.name, git_token))
                    }
                    ReleaseGitBackendKind::Gitlab => {
                        GitBackend::Gitlab(GitLab::new(repo_url, git_token)?)
                    }
                },
            };
            Some(release)
        } else {
            None
        };
        let mut req = ReleaseRequest::new(metadata).with_dry_run(self.dry_run);

        if let Some(registry) = self.registry {
            req = req.with_registry(registry);
        }
        if let Some(token) = self.token {
            req = req.with_token(SecretString::from(token));
        }
        if let Some(repo_url) = self.repo_url {
            req = req.with_repo_url(repo_url);
        }
        if let Some(git_release) = git_release {
            req = req.with_git_release(git_release);
        }
        if let Some(release_always) = config.workspace.release_always {
            req = req.with_release_always(release_always);
        }

        req = req.with_publish_timeout(config.workspace.publish_timeout()?);

        req = config.fill_release_config(self.allow_dirty, self.no_verify, req);

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

    #[test]
    fn input_generates_correct_release_request() {
        let config = r#"
            [workspace]
            dependencies_update = false
            changelog_config = "../git-cliff.toml"
            allow_dirty = false
            repo_url = "https://github.com/release-plz/release-plz"
            publish_allow_dirty = true
            git_release_enable = true
            git_release_type = "prod"
            git_release_draft = false
        "#;

        let release_args = default_args();
        let config: Config = toml::from_str(config).unwrap();
        let actual_request = release_args
            .release_request(&config, fake_metadata())
            .unwrap();
        assert!(actual_request.allow_dirty("aaa"));
    }

    #[test]
    fn package_config_is_overriden() {
        let config = r#"
            [workspace]
            publish_allow_dirty = false
            publish_no_verify = true

            [[package]]
            name = "aaa"
            publish_allow_dirty = true
            publish_features = ["a", "b", "c"]
        "#;

        let release_args = default_args();
        let config: Config = toml::from_str(config).unwrap();
        let actual_request = release_args
            .release_request(&config, fake_metadata())
            .unwrap();
        assert!(actual_request.allow_dirty("aaa"));
        assert!(actual_request.no_verify("aaa"));
        assert_eq!(actual_request.features("aaa"), &["a", "b", "c"]);
    }

    fn default_args() -> Release {
        Release {
            allow_dirty: false,
            no_verify: false,
            manifest_path: None,
            registry: None,
            token: None,
            dry_run: false,
            repo_url: None,
            git_token: None,
            backend: ReleaseGitBackendKind::Github,
            config: None,
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
        let expected = release_plz_core::ReleaseConfig::default();
        assert_eq!(pkg_config, expected);
        assert!(pkg_config.git_release().is_enabled());
    }
}
