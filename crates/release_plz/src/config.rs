use anyhow::Context as _;
use cargo_metadata::camino::Utf8Path;
use cargo_utils::to_utf8_pathbuf;
use release_plz_core::{
    GitReleaseConfig, ReleaseRequest, fs_utils::to_utf8_path, set_version::SetVersionRequest,
    update_request::UpdateRequest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use url::Url;

use crate::changelog_config::ChangelogCfg;

/// You can find the documentation of the configuration file
/// [here](https://release-plz.dev/docs/config).
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("$id" = "https://raw.githubusercontent.com/release-plz/release-plz/main/.schema/latest.json"))]
pub struct Config {
    /// # Workspace
    /// Global configuration. Applied to all packages by default.
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub changelog: ChangelogCfg,
    /// # Package
    /// Package-specific configuration. This overrides `workspace`.
    /// Not all settings of `workspace` can be overridden.
    #[serde(default)]
    package: Vec<PackageSpecificConfigWithName>,
}

impl Config {
    /// Package-specific configurations.
    /// Returns `<package name, package config>`.
    fn packages(&self) -> HashMap<&str, &PackageSpecificConfig> {
        self.package
            .iter()
            .map(|p| (p.name.as_str(), &p.config))
            .collect()
    }

    pub fn fill_update_config(
        &self,
        is_changelog_update_disabled: bool,
        update_request: UpdateRequest,
    ) -> UpdateRequest {
        let mut default_update_config = self.workspace.packages_defaults.clone();
        if is_changelog_update_disabled {
            default_update_config.changelog_update = false.into();
        }
        let mut update_request =
            update_request.with_default_package_config(default_update_config.into());
        for (package, config) in self.packages() {
            let mut update_config = config.clone();
            update_config = update_config.merge(self.workspace.packages_defaults.clone());
            if is_changelog_update_disabled {
                update_config.common.changelog_update = false.into();
            }
            update_request = update_request.with_package_config(package, update_config.into());
        }
        update_request
    }

    pub fn fill_set_version_config(
        &self,
        set_version_request: &mut SetVersionRequest,
    ) -> anyhow::Result<()> {
        for (package, config) in self.packages() {
            if let Some(changelog_path) = config.common.changelog_path.clone() {
                let changelog_path = to_utf8_pathbuf(changelog_path)?;
                set_version_request.set_changelog_path(package, changelog_path);
            }
        }
        Ok(())
    }

    pub fn fill_release_config(
        &self,
        allow_dirty: bool,
        no_verify: bool,
        release_request: ReleaseRequest,
    ) -> ReleaseRequest {
        let mut default_config = self.workspace.packages_defaults.clone();
        if no_verify {
            default_config.publish_no_verify = Some(true);
        }
        if allow_dirty {
            default_config.publish_allow_dirty = Some(true);
        }
        let mut release_request =
            release_request.with_default_package_config(default_config.into());

        for (package, config) in self.packages() {
            let mut release_config = config.clone();
            release_config = release_config.merge(self.workspace.packages_defaults.clone());

            if no_verify {
                release_config.common.publish_no_verify = Some(true);
            }
            if allow_dirty {
                release_config.common.publish_allow_dirty = Some(true);
            }
            release_request =
                release_request.with_package_config(package, release_config.common.into());
        }
        release_request
    }
}

/// Config at the `[workspace]` level.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    /// Configuration applied at the `[[package]]` level, too.
    #[serde(flatten)]
    pub packages_defaults: PackageConfig,
    /// # Allow Dirty
    /// - If `true`, allow dirty working directories to be updated. The uncommitted changes will be part of the update.
    /// - If `false` or [`Option::None`], the command will fail if the working directory is dirty.
    pub allow_dirty: Option<bool>,
    /// # Changelog Config
    /// Path to the git cliff configuration file. Defaults to the `keep a changelog` configuration.
    pub changelog_config: Option<PathBuf>,
    /// # Dependencies Update
    /// - If `true`, update all the dependencies in the Cargo.lock file by running `cargo update`.
    /// - If `false` or [`Option::None`], only update the workspace packages by running `cargo update --workspace`.
    pub dependencies_update: Option<bool>,
    /// # PR Name
    /// Tera template of the pull request's name created by release-plz.
    pub pr_name: Option<String>,
    /// # PR Body
    /// Tera template of the pull request's body created by release-plz.
    pub pr_body: Option<String>,
    /// # PR Draft
    /// If `true`, the created release PR will be marked as a draft.
    #[serde(default)]
    pub pr_draft: bool,
    /// # PR Labels
    /// Labels to add to the release PR.
    #[serde(default)]
    pub pr_labels: Vec<String>,
    /// # PR Branch Prefix
    /// Prefix for the PR Branch
    pub pr_branch_prefix: Option<String>,
    /// # Publish Timeout
    /// Timeout for the publishing process
    pub publish_timeout: Option<String>,
    /// # Repo URL
    /// GitHub/Gitea/GitLab repository url where your project is hosted.
    /// It is used to generate the changelog release link.
    /// It defaults to the url of the default remote.
    pub repo_url: Option<Url>,
    /// # Release Commits
    /// Prepare release only if at least one commit respects this regex.
    pub release_commits: Option<String>,
    /// # Release always
    /// - If true, release-plz release will try to release your packages every time you run it
    ///   (e.g. on every commit in the main branch). *(Default)*.
    /// - If false, `release-plz release` will try release your packages only when you merge the
    ///   release pr.
    ///   Use this if you want to commit your packages and publish them later.
    ///   To determine if a pr is a release-pr, release-plz will check if the branch of the PR starts with
    ///   `release-plz-`. So if you want to create a PR that should trigger a release
    ///   (e.g. when you fix the CI), use this branch name format (e.g. `release-plz-fix-ci`).
    pub release_always: Option<bool>,

    /// Use git tags for release information
    /// Default: false
    ///
    /// If true, release-plz will use git tags to determine what the latest version of the package
    /// is (i.e newest version is v0.1.3 and is associated with commit ac83762)
    /// If false, release-plz will use crates.io release information to get the latest version
    pub git_only: Option<bool>,

    /// Literal string prefix for release tags when git_only is enabled.
    /// Optional. For example: "v" matches "v1.2.3", "package-v" matches "package-v1.2.3".
    /// Default: empty string (tags like "1.2.3")
    pub git_only_release_tag_prefix: Option<String>,

    /// Literal string suffix for release tags when git_only is enabled.
    /// Optional. For example: "-release" matches "v1.2.3-release".
    /// Default: empty string (no suffix)
    pub git_only_release_tag_suffix: Option<String>,
}

impl Workspace {
    /// Get the publish timeout. Defaults to 30 minutes.
    pub fn publish_timeout(&self) -> anyhow::Result<Duration> {
        let publish_timeout = self.publish_timeout.as_deref().unwrap_or("30m");
        parse_duration(publish_timeout)
            .with_context(|| format!("invalid publish_timeout '{publish_timeout}'"))
    }
}

/// Parse the duration from the input string.
/// The code is simple enough that it's not worth adding a dependency.
fn parse_duration(input: &str) -> anyhow::Result<Duration> {
    let (number_str, unit) = parse_duration_unit(input)?;

    let number = number_str
        .parse::<u64>()
        .context("invalid duration number")?;

    match unit {
        DurationUnit::Seconds => Ok(Duration::from_secs(number)),
        DurationUnit::Minutes => Ok(Duration::from_secs(number * 60)),
        DurationUnit::Hours => Ok(Duration::from_secs(number * 60 * 60)),
    }
}

enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
}

fn parse_duration_unit(input: &str) -> anyhow::Result<(&str, DurationUnit)> {
    if let Some(stripped) = input.strip_suffix('s') {
        Ok((stripped, DurationUnit::Seconds))
    } else if let Some(stripped) = input.strip_suffix('m') {
        Ok((stripped, DurationUnit::Minutes))
    } else if let Some(stripped) = input.strip_suffix('h') {
        Ok((stripped, DurationUnit::Hours))
    } else if let Some(last_char) = input.chars().last() {
        anyhow::ensure!(
            !last_char.is_ascii_alphabetic(),
            "'{last_char}' is not a valid time unit. Valid units are: 's', 'm' and 'h'"
        );
        // Default to seconds if no unit specified
        Ok((input, DurationUnit::Seconds))
    } else {
        anyhow::bail!("input cannot be empty");
    }
}

/// Config at the `[[package]]` level.
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageSpecificConfig {
    /// Configuration that can be specified at the `[workspace]` level, too.
    #[serde(flatten)]
    common: PackageConfig,
    /// # Changelog Include
    /// List of package names.
    /// Include the changelogs of these packages in the changelog of the current package.
    changelog_include: Option<Vec<String>>,
    /// # Version group
    /// The name of a group of packages that needs to have the same version.
    version_group: Option<String>,
}

impl PackageSpecificConfig {
    /// Merge the package-specific configuration with the global configuration.
    pub fn merge(self, default: PackageConfig) -> Self {
        Self {
            common: self.common.merge(default),
            changelog_include: self.changelog_include,
            version_group: self.version_group,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct PackageSpecificConfigWithName {
    pub name: String,
    #[serde(flatten)]
    pub config: PackageSpecificConfig,
}

impl From<PackageConfig> for release_plz_core::ReleaseConfig {
    fn from(value: PackageConfig) -> Self {
        let is_publish_enabled = value.publish != Some(false);
        let is_git_tag_enabled = value.git_tag_enable != Some(false);
        let git_tag_name = value.git_tag_name.clone();
        let release = value.release != Some(false);
        let mut cfg = Self::default()
            .with_publish(release_plz_core::PublishConfig::enabled(is_publish_enabled))
            .with_git_release(git_release(&value))
            .with_git_tag(
                release_plz_core::GitTagConfig::enabled(is_git_tag_enabled)
                    .set_name_template(git_tag_name),
            )
            .with_release(release);

        if let Some(changelog_update) = value.changelog_update {
            cfg = cfg.with_changelog_update(changelog_update);
        }
        if let Some(changelog_path) = value.changelog_path {
            cfg = cfg.with_changelog_path(to_utf8_pathbuf(changelog_path).unwrap());
        }
        if let Some(no_verify) = value.publish_no_verify {
            cfg = cfg.with_no_verify(no_verify);
        }
        if let Some(features) = value.publish_features {
            cfg = cfg.with_features(features);
        }
        if let Some(all_features) = value.publish_all_features {
            cfg = cfg.with_all_features(all_features);
        }
        if let Some(allow_dirty) = value.publish_allow_dirty {
            cfg = cfg.with_allow_dirty(allow_dirty);
        }
        cfg
    }
}

fn git_release(config: &PackageConfig) -> GitReleaseConfig {
    let is_git_release_enabled = config.git_release_enable != Some(false);
    let git_release_type: release_plz_core::ReleaseType = config
        .git_release_type
        .map(|release_type| release_type.into())
        .unwrap_or_default();
    let is_git_release_draft = config.git_release_draft == Some(true);
    let git_release_name = config.git_release_name.clone();
    let git_release_body = config.git_release_body.clone();
    let mut git_release = release_plz_core::GitReleaseConfig::enabled(is_git_release_enabled)
        .set_draft(is_git_release_draft)
        .set_release_type(git_release_type)
        .set_name_template(git_release_name)
        .set_body_template(git_release_body);

    if config.git_release_latest == Some(false) {
        git_release = git_release.set_latest(false);
    }

    git_release
}

/// Configuration that can be specified both at the `[workspace]` and at the `[[package]]` level.
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Default, Clone, JsonSchema)]
pub struct PackageConfig {
    /// # Changelog Path
    /// Normally the changelog is placed in the same directory of the Cargo.toml file.
    /// The user can provide a custom path here.
    /// `changelog_path` is propagated to the commands:
    /// `update`, `release-pr` and `release`.
    pub changelog_path: Option<PathBuf>,
    /// # Changelog Update
    /// Whether to create/update changelog or not.
    /// If unspecified, the changelog is updated.
    pub changelog_update: Option<bool>,
    /// # Features Always Increment Minor Version
    /// - If `true`, feature commits will always bump the minor version, even in 0.x releases.
    /// - If `false` (default), feature commits will only bump the minor version starting with 1.x releases.
    pub features_always_increment_minor: Option<bool>,
    /// # Git Release Enable
    /// Publish the GitHub/Gitea/GitLab release for the created git tag.
    /// Enabled by default.
    pub git_release_enable: Option<bool>,
    /// # Git Release Body
    /// Tera template of the git release body created by release-plz.
    pub git_release_body: Option<String>,
    /// # Git Release Type
    /// Whether to mark the created release as not ready for production.
    pub git_release_type: Option<ReleaseType>,
    /// # Git Release Draft
    /// If true, will not auto-publish the release.
    pub git_release_draft: Option<bool>,
    /// # Git Release Latest
    /// If true, will set the git release as latest.
    pub git_release_latest: Option<bool>,
    /// # Git Release Name
    /// Tera template of the git release name created by release-plz.
    pub git_release_name: Option<String>,
    /// # Git Tag Enable
    /// Publish the git tag for the new package version.
    /// Enabled by default.
    pub git_tag_enable: Option<bool>,
    /// # Git Tag Name
    /// Tera template of the git tag name created by release-plz.
    pub git_tag_name: Option<String>,
    /// # Publish
    /// If `false`, don't run `cargo publish`.
    pub publish: Option<bool>,
    /// # Publish Allow Dirty
    /// If `true`, add the `--allow-dirty` flag to the `cargo publish` command.
    pub publish_allow_dirty: Option<bool>,
    /// # Publish No Verify
    /// If `true`, add the `--no-verify` flag to the `cargo publish` command.
    pub publish_no_verify: Option<bool>,
    /// # Publish Features
    /// If `["a", "b", "c"]`, add the `--features=a,b,c` flag to the `cargo publish` command.
    pub publish_features: Option<Vec<String>>,
    /// # Publish All Features
    /// If `true`, add the `--all-features` flag to the `cargo publish` command.
    pub publish_all_features: Option<bool>,
    /// # Semver Check
    /// Controls when to run cargo-semver-checks.
    /// If unspecified, run cargo-semver-checks if the package is a library.
    pub semver_check: Option<bool>,
    /// # Release
    /// Used to toggle off the update/release process for a workspace or package.
    pub release: Option<bool>,
    /// # Git Only
    /// If true, use git tags to determine the latest version instead of the registry.
    /// If unspecified at package level, inherits from workspace config.
    pub git_only: Option<bool>,
    /// # Git Only Release Tag Prefix
    /// Literal string prefix for release tags when git_only is enabled.
    /// If unspecified at package level, inherits from workspace config.
    pub git_only_release_tag_prefix: Option<String>,
    /// # Git Only Release Tag Suffix
    /// Literal string suffix for release tags when git_only is enabled.
    /// If unspecified at package level, inherits from workspace config.
    pub git_only_release_tag_suffix: Option<String>,
}

impl From<PackageConfig> for release_plz_core::UpdateConfig {
    fn from(config: PackageConfig) -> Self {
        Self {
            semver_check: config.semver_check != Some(false),
            changelog_update: config.changelog_update != Some(false),
            release: config.release != Some(false),
            publish: config.publish != Some(false),
            tag_name_template: config.git_tag_name,
            features_always_increment_minor: config.features_always_increment_minor == Some(true),
            changelog_path: config.changelog_path.map(|p| to_utf8_pathbuf(p).unwrap()),
            git_only: config.git_only,
            git_only_release_tag_prefix: config.git_only_release_tag_prefix,
            git_only_release_tag_suffix: config.git_only_release_tag_suffix,
        }
    }
}

impl From<PackageSpecificConfig> for release_plz_core::PackageUpdateConfig {
    fn from(config: PackageSpecificConfig) -> Self {
        Self {
            generic: config.common.into(),
            changelog_include: config.changelog_include.unwrap_or_default(),
            version_group: config.version_group,
        }
    }
}

impl PackageConfig {
    /// Merge the package-specific configuration with the global configuration.
    pub fn merge(self, default: Self) -> Self {
        Self {
            semver_check: self.semver_check.or(default.semver_check),
            changelog_path: self.changelog_path.or(default.changelog_path),
            changelog_update: self.changelog_update.or(default.changelog_update),
            features_always_increment_minor: self
                .features_always_increment_minor
                .or(default.features_always_increment_minor),
            git_release_enable: self.git_release_enable.or(default.git_release_enable),
            git_release_type: self.git_release_type.or(default.git_release_type),
            git_release_draft: self.git_release_draft.or(default.git_release_draft),
            git_release_latest: self.git_release_latest.or(default.git_release_latest),
            git_release_name: self.git_release_name.or(default.git_release_name),
            git_release_body: self.git_release_body.or(default.git_release_body),

            publish: self.publish.or(default.publish),
            publish_allow_dirty: self.publish_allow_dirty.or(default.publish_allow_dirty),
            publish_no_verify: self.publish_no_verify.or(default.publish_no_verify),
            publish_features: self.publish_features.or(default.publish_features),
            publish_all_features: self.publish_all_features.or(default.publish_all_features),
            git_tag_enable: self.git_tag_enable.or(default.git_tag_enable),
            git_tag_name: self.git_tag_name.or(default.git_tag_name),
            release: self.release.or(default.release),
            git_only: self.git_only.or(default.git_only),
            git_only_release_tag_prefix: self
                .git_only_release_tag_prefix
                .or(default.git_only_release_tag_prefix),
            git_only_release_tag_suffix: self
                .git_only_release_tag_suffix
                .or(default.git_only_release_tag_suffix),
        }
    }

    pub fn changelog_path(&self) -> Option<&Utf8Path> {
        self.changelog_path
            .as_ref()
            .map(|p| to_utf8_path(p.as_ref()).unwrap())
    }
}

#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    /// # Prod
    /// Will mark the release as ready for production.
    #[default]
    Prod,
    /// # Pre
    /// Will mark the release as not ready for production.
    /// I.e. as pre-release.
    Pre,
    /// # Auto
    /// Will mark the release as not ready for production
    /// in case there is a semver pre-release in the tag e.g. v1.0.0-rc1.
    /// Otherwise, will mark the release as ready for production.
    Auto,
}

impl From<ReleaseType> for release_plz_core::ReleaseType {
    fn from(value: ReleaseType) -> Self {
        match value {
            ReleaseType::Prod => Self::Prod,
            ReleaseType::Pre => Self::Pre,
            ReleaseType::Auto => Self::Auto,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_WORKSPACE_CONFIG: &str = r#"
        [workspace]
        dependencies_update = false
        allow_dirty = false
        changelog_config = "../git-cliff.toml"
        repo_url = "https://github.com/release-plz/release-plz"
        git_release_enable = true
        git_release_type = "prod"
        git_release_draft = false
        pr_branch_prefix = "f-"
        publish_timeout = "10m"
        release_commits = "^feat:"
    "#;

    const BASE_PACKAGE_CONFIG: &str = r#"
        [[package]]
        name = "crate1"
    "#;

    fn create_base_workspace_config() -> Config {
        Config {
            changelog: ChangelogCfg::default(),
            workspace: Workspace {
                dependencies_update: Some(false),
                changelog_config: Some("../git-cliff.toml".into()),
                allow_dirty: Some(false),
                repo_url: Some(
                    "https://github.com/release-plz/release-plz"
                        .parse()
                        .unwrap(),
                ),
                packages_defaults: PackageConfig {
                    semver_check: None,
                    changelog_update: None,
                    git_release_enable: Some(true),
                    git_release_type: Some(ReleaseType::Prod),
                    git_release_draft: Some(false),
                    ..Default::default()
                },
                pr_name: None,
                pr_body: None,
                pr_draft: false,
                pr_labels: vec![],
                pr_branch_prefix: Some("f-".to_string()),
                publish_timeout: Some("10m".to_string()),
                release_commits: Some("^feat:".to_string()),
                release_always: None,
                git_only: None,
                git_only_release_tag_prefix: None,
                git_only_release_tag_suffix: None,
            },
            package: [].into(),
        }
    }

    fn create_base_package_config() -> PackageSpecificConfigWithName {
        PackageSpecificConfigWithName {
            name: "crate1".to_string(),
            config: PackageSpecificConfig {
                common: PackageConfig {
                    semver_check: None,
                    changelog_update: None,
                    git_release_enable: None,
                    git_release_type: None,
                    git_release_draft: None,
                    ..Default::default()
                },
                changelog_include: None,
                version_group: None,
            },
        }
    }

    #[test]
    fn config_without_update_config_is_deserialized() {
        let expected_config = create_base_workspace_config();

        let config: Config = toml::from_str(BASE_WORKSPACE_CONFIG).unwrap();
        assert_eq!(config, expected_config);
    }

    #[test]
    fn config_is_deserialized() {
        let config = &format!(
            "{BASE_WORKSPACE_CONFIG}\
            changelog_update = true"
        );

        let mut expected_config = create_base_workspace_config();
        expected_config.workspace.packages_defaults.changelog_update = true.into();

        let config: Config = toml::from_str(config).unwrap();
        assert_eq!(config, expected_config);
    }

    fn config_package_release_is_deserialized(config_flag: &str, expected_value: bool) {
        let config = &format!(
            "{BASE_WORKSPACE_CONFIG}\n{BASE_PACKAGE_CONFIG}\
            release = {config_flag}"
        );

        let mut expected_config = create_base_workspace_config();
        let mut package_config = create_base_package_config();
        package_config.config.common.release = expected_value.into();
        expected_config.package = [package_config].into();

        let config: Config = toml::from_str(config).unwrap();
        assert_eq!(config, expected_config);
    }

    #[test]
    fn config_package_release_is_deserialized_true() {
        config_package_release_is_deserialized("true", true);
    }

    #[test]
    fn config_package_release_is_deserialized_false() {
        config_package_release_is_deserialized("false", false);
    }

    fn config_workspace_release_is_deserialized(config_flag: &str, expected_value: bool) {
        let config = &format!(
            "{BASE_WORKSPACE_CONFIG}\
            release = {config_flag}"
        );

        let mut expected_config = create_base_workspace_config();
        expected_config.workspace.packages_defaults.release = expected_value.into();

        let config: Config = toml::from_str(config).unwrap();
        assert_eq!(config, expected_config);
    }

    #[test]
    fn config_workspace_release_is_deserialized_true() {
        config_workspace_release_is_deserialized("true", true);
    }

    #[test]
    fn config_workspace_release_is_deserialized_false() {
        config_workspace_release_is_deserialized("false", false);
    }

    #[test]
    fn config_is_serialized() {
        let config = Config {
            changelog: ChangelogCfg::default(),
            workspace: Workspace {
                dependencies_update: None,
                changelog_config: Some("../git-cliff.toml".into()),
                allow_dirty: None,
                repo_url: Some(
                    "https://github.com/release-plz/release-plz"
                        .parse()
                        .unwrap(),
                ),
                pr_name: None,
                pr_body: None,
                pr_draft: false,
                pr_labels: vec!["label1".to_string()],
                pr_branch_prefix: Some("f-".to_string()),
                packages_defaults: PackageConfig {
                    semver_check: None,
                    changelog_update: true.into(),
                    git_release_enable: true.into(),
                    git_release_type: Some(ReleaseType::Prod),
                    git_release_draft: Some(false),
                    release: Some(true),
                    changelog_path: Some("./CHANGELOG.md".into()),
                    ..Default::default()
                },
                publish_timeout: Some("10m".to_string()),
                release_commits: Some("^feat:".to_string()),
                release_always: None,
                git_only: None,
                git_only_release_tag_prefix: None,
                git_only_release_tag_suffix: None,
            },
            package: [PackageSpecificConfigWithName {
                name: "crate1".to_string(),
                config: PackageSpecificConfig {
                    common: PackageConfig {
                        semver_check: Some(false),
                        changelog_update: true.into(),
                        git_release_enable: true.into(),
                        git_release_type: Some(ReleaseType::Prod),
                        git_release_draft: Some(false),
                        release: Some(false),
                        ..Default::default()
                    },
                    changelog_include: Some(vec!["pkg1".to_string()]),
                    version_group: None,
                },
            }]
            .into(),
        };

        expect_test::expect![[r#"
            [workspace]
            changelog_path = "./CHANGELOG.md"
            changelog_update = true
            git_release_enable = true
            git_release_type = "prod"
            git_release_draft = false
            release = true
            changelog_config = "../git-cliff.toml"
            pr_draft = false
            pr_labels = ["label1"]
            pr_branch_prefix = "f-"
            publish_timeout = "10m"
            repo_url = "https://github.com/release-plz/release-plz"
            release_commits = "^feat:"

            [changelog]

            [[package]]
            name = "crate1"
            changelog_update = true
            git_release_enable = true
            git_release_type = "prod"
            git_release_draft = false
            semver_check = false
            release = false
            changelog_include = ["pkg1"]
        "#]]
        .assert_eq(&toml::to_string(&config).unwrap());
    }

    #[test]
    fn wrong_config_section_is_not_deserialized() {
        let config = "[unknown]";

        let error = toml::from_str::<Config>(config).unwrap_err().to_string();
        expect_test::expect![[r"
            TOML parse error at line 1, column 2
              |
            1 | [unknown]
              |  ^^^^^^^
            unknown field `unknown`, expected one of `workspace`, `changelog`, `package`
        "]]
        .assert_eq(&error);
    }

    #[test]
    fn wrong_workspace_section_is_not_deserialized() {
        let config = r"
[workspace]
unknown = false
allow_dirty = true";

        let error = toml::from_str::<Config>(config).unwrap_err().to_string();
        expect_test::expect![[r"
            TOML parse error at line 2, column 1
              |
            2 | [workspace]
              | ^^^^^^^^^^^
            unknown field `unknown`
        "]]
        .assert_eq(&error);
    }

    #[test]
    fn wrong_changelog_section_is_not_deserialized() {
        let config = r"
[changelog]
trim = true
unknown = false";

        let error = toml::from_str::<Config>(config).unwrap_err().to_string();
        expect_test::expect![[r"
            TOML parse error at line 4, column 1
              |
            4 | unknown = false
              | ^^^^^^^
            unknown field `unknown`, expected one of `header`, `body`, `trim`, `commit_preprocessors`, `postprocessors`, `sort_commits`, `link_parsers`, `commit_parsers`, `protect_breaking_commits`, `tag_pattern`
        "]]
        .assert_eq(&error);
    }

    #[test]
    fn wrong_package_section_is_not_deserialized() {
        let config = r#"
[[package]]
name = "crate1"
unknown = false"#;

        let error = toml::from_str::<Config>(config).unwrap_err().to_string();
        expect_test::expect![[r"
            TOML parse error at line 2, column 1
              |
            2 | [[package]]
              | ^^^^^^^^^^^
            unknown field `unknown`
        "]]
        .assert_eq(&error);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60)); // Default to seconds
        assert_eq!(
            parse_duration("").unwrap_err().to_string(),
            "input cannot be empty"
        );
        assert_eq!(
            parse_duration("30x").unwrap_err().to_string(),
            "'x' is not a valid time unit. Valid units are: 's', 'm' and 'h'"
        );
        assert_eq!(
            parse_duration("-30s").unwrap_err().to_string(),
            "invalid duration number"
        );
    }
}
