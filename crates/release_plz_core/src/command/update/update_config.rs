use cargo_metadata::camino::Utf8PathBuf;
use next_version::VersionUpdater;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateConfig {
    /// This path needs to be a relative path to the Cargo.toml of the project.
    /// I.e. if you have a workspace, it needs to be relative to the workspace root.
    pub changelog_path: Option<Utf8PathBuf>,
    /// Controls when to run cargo-semver-checks.
    /// Note: You can only run cargo-semver-checks if the package contains a library.
    ///       For example, if it has a `lib.rs` file.
    pub semver_check: bool,
    /// Whether to create/update changelog or not.
    /// Default: `true`.
    pub changelog_update: bool,
    /// High-level toggle to process this package or ignore it.
    pub release: bool,
    /// Whether to publish this package to a registry.
    /// Default: `true`.
    pub publish: bool,
    /// - If `true`, feature commits will always bump the minor version, even in 0.x releases.
    /// - If `false` (default), feature commits will only bump the minor version starting with
    ///   1.x releases.
    pub features_always_increment_minor: bool,
    /// Template for the git tag created by release-plz.
    pub tag_name_template: Option<String>,
    /// Custom regex to match commit types that should trigger a minor version increment.
    pub custom_minor_increment_regex: Option<String>,
    /// Custom regex to match commit types that should trigger a major version increment.
    pub custom_major_increment_regex: Option<String>,
    /// Whether to use git tags instead of registry for determining package versions.
    pub git_only: Option<bool>,
}

/// Package-specific config
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageUpdateConfig {
    /// config that can be applied by default to all packages.
    pub generic: UpdateConfig,
    /// List of package names.
    /// Include the changelogs of these packages in the changelog of the current package.
    pub changelog_include: Vec<String>,
    pub version_group: Option<String>,
}

impl From<UpdateConfig> for PackageUpdateConfig {
    fn from(config: UpdateConfig) -> Self {
        Self {
            generic: config,
            changelog_include: vec![],
            version_group: None,
        }
    }
}

impl PackageUpdateConfig {
    pub fn semver_check(&self) -> bool {
        self.generic.semver_check
    }

    pub fn should_update_changelog(&self) -> bool {
        self.generic.changelog_update
    }

    pub fn should_publish(&self) -> bool {
        self.generic.publish
    }

    pub fn git_only(&self) -> Option<bool> {
        self.generic.git_only
    }
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            semver_check: true,
            changelog_update: true,
            release: true,
            publish: true,
            features_always_increment_minor: false,
            git_only: None,
            tag_name_template: None,
            changelog_path: None,
            custom_minor_increment_regex: None,
            custom_major_increment_regex: None,
        }
    }
}

impl UpdateConfig {
    pub fn with_semver_check(self, semver_check: bool) -> Self {
        Self {
            semver_check,
            ..self
        }
    }

    pub fn with_features_always_increment_minor(
        self,
        features_always_increment_minor: bool,
    ) -> Self {
        Self {
            features_always_increment_minor,
            ..self
        }
    }

    pub fn with_changelog_update(self, changelog_update: bool) -> Self {
        Self {
            changelog_update,
            ..self
        }
    }

    pub fn with_publish(self, publish: bool) -> Self {
        Self { publish, ..self }
    }

    pub fn version_updater(&self) -> Result<VersionUpdater, regex::Error> {
        let mut updater = VersionUpdater::default()
            .with_features_always_increment_minor(self.features_always_increment_minor);
        if let Some(regex) = &self.custom_minor_increment_regex {
            updater = updater.with_custom_minor_increment_regex(regex)?;
        }
        if let Some(regex) = &self.custom_major_increment_regex {
            updater = updater.with_custom_major_increment_regex(regex)?;
        }
        Ok(updater)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_metadata::semver::Version;

    #[test]
    fn version_updater_with_custom_minor_regex() {
        let config = UpdateConfig {
            custom_minor_increment_regex: Some("minor|enhancement".to_string()),
            ..Default::default()
        };
        let updater = config.version_updater().unwrap();
        // Test that the updater correctly uses the custom regex.
        let commits = ["enhancement: add new feature"];
        let version = Version::new(1, 2, 3);
        let new_version = updater.increment(&version, commits);
        assert_eq!(new_version, Version::new(1, 3, 0));
    }

    #[test]
    fn version_updater_with_invalid_regex() {
        let config = UpdateConfig {
            custom_minor_increment_regex: Some("[invalid".to_string()),
            ..Default::default()
        };
        assert!(config.version_updater().is_err());
    }

    #[test]
    fn version_updater_without_custom_regex() {
        let config = UpdateConfig::default();
        let updater = config.version_updater().unwrap();
        // Test that the updater works normally without custom regex.
        let commits = ["some change"];
        let version = Version::new(1, 2, 3);
        let new_version = updater.increment(&version, commits);
        assert_eq!(new_version, Version::new(1, 2, 4));
    }

    #[test]
    fn version_updater_with_custom_major_regex() {
        let config = UpdateConfig {
            custom_major_increment_regex: Some("major|breaking".to_string()),
            ..Default::default()
        };
        let updater = config.version_updater().unwrap();
        // Test that the updater correctly uses the custom regex.
        let commits = ["breaking: remove old API"];
        let version = Version::new(1, 2, 3);
        let new_version = updater.increment(&version, commits);
        assert_eq!(new_version, Version::new(2, 0, 0));
    }

    #[test]
    fn version_updater_with_invalid_major_regex() {
        let config = UpdateConfig {
            custom_major_increment_regex: Some("[invalid".to_string()),
            ..Default::default()
        };
        assert!(config.version_updater().is_err());
    }
}
