use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use anyhow::Context as _;
use cargo_metadata::{
    Metadata, Package,
    camino::{Utf8Path, Utf8PathBuf},
};
use regex::Regex;

use crate::{ChangelogRequest, GitClient, GitForge, PackagePath as _, RepoUrl, fs_utils};

use super::update_config::{PackageUpdateConfig, UpdateConfig};

#[derive(Debug, Clone)]
pub struct UpdateRequest {
    /// The manifest of the project you want to update.
    local_manifest: Utf8PathBuf,
    /// Cargo metadata.
    metadata: Metadata,
    /// Manifest of the project containing packages at the versions published in the Cargo registry.
    registry_manifest: Option<Utf8PathBuf>,
    /// Update just this package.
    single_package: Option<String>,
    /// Changelog options.
    changelog_req: ChangelogRequest,
    /// Registry where the packages are stored.
    /// The registry name needs to be present in the Cargo config.
    /// If unspecified, crates.io is used.
    registry: Option<String>,
    /// - If true, update all the dependencies in Cargo.lock by running `cargo update`.
    /// - If false, updates the workspace packages in Cargo.lock by running `cargo update --workspace`.
    dependencies_update: bool,
    /// Allow dirty working directories to be updated.
    /// The uncommitted changes will be part of the update.
    allow_dirty: bool,
    /// Repository Url. If present, the new changelog entry contains a link to the diff between the old and new version.
    /// Format: `https://{repo_host}/{repo_owner}/{repo_name}/compare/{old_tag}...{new_tag}`.
    repo_url: Option<RepoUrl>,
    /// Package-specific configurations.
    packages_config: PackagesConfig,
    /// Release Commits
    /// Prepare release only if at least one commit respects a regex.
    release_commits: Option<Regex>,
    git: Option<GitForge>,

    /// Use git tags to determine latest package release
    git_only: Option<bool>,

    /// Literal string prefix for release tags when git_only is enabled
    git_only_release_tag_prefix: Option<String>,

    /// Literal string suffix for release tags when git_only is enabled
    git_only_release_tag_suffix: Option<String>,
}

impl UpdateRequest {
    pub fn new(metadata: Metadata) -> anyhow::Result<Self> {
        let local_manifest = cargo_utils::workspace_manifest(&metadata);
        let local_manifest = cargo_utils::canonical_local_manifest(local_manifest.as_ref())?;
        Ok(Self {
            local_manifest,
            metadata,
            registry_manifest: None,
            single_package: None,
            changelog_req: ChangelogRequest::default(),
            registry: None,
            dependencies_update: false,
            allow_dirty: false,
            repo_url: None,
            packages_config: PackagesConfig::default(),
            release_commits: None,
            git: None,
            git_only: None,
            git_only_release_tag_prefix: None,
            git_only_release_tag_suffix: None,
        })
    }

    pub fn changelog_path(&self, package: &Package) -> Utf8PathBuf {
        let config = self.get_package_config(&package.name);
        config
            .generic
            .changelog_path
            .map(|p| self.local_manifest.parent().unwrap().join(p))
            .unwrap_or_else(|| {
                package
                    .package_path()
                    .expect("can't determine package path")
                    .join(crate::CHANGELOG_FILENAME)
            })
    }

    pub fn git_client(&self) -> anyhow::Result<Option<GitClient>> {
        self.git
            .as_ref()
            .map(|git| GitClient::new(git.clone()))
            .transpose()
    }

    pub fn cargo_metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn set_local_manifest(self, local_manifest: impl AsRef<Path>) -> anyhow::Result<Self> {
        Ok(Self {
            local_manifest: cargo_utils::canonical_local_manifest(local_manifest.as_ref())?,
            ..self
        })
    }

    pub fn with_git_client(self, git: GitForge) -> Self {
        Self {
            git: Some(git),
            ..self
        }
    }

    pub fn with_registry_manifest_path(self, registry_manifest: &Utf8Path) -> anyhow::Result<Self> {
        let registry_manifest = fs_utils::canonicalize_utf8(registry_manifest)?;
        Ok(Self {
            registry_manifest: Some(registry_manifest),
            ..self
        })
    }

    pub fn with_changelog_req(self, changelog_req: ChangelogRequest) -> Self {
        Self {
            changelog_req,
            ..self
        }
    }

    /// Set update config for all packages.
    pub fn with_default_package_config(mut self, config: UpdateConfig) -> Self {
        self.packages_config.set_default(config);
        self
    }

    /// Set update config for a specific package.
    pub fn with_package_config(
        mut self,
        package: impl Into<String>,
        config: PackageUpdateConfig,
    ) -> Self {
        self.packages_config.set(package.into(), config);
        self
    }

    pub fn get_package_config(&self, package: &str) -> PackageUpdateConfig {
        self.packages_config.get(package)
    }

    pub fn with_registry(self, registry: String) -> Self {
        Self {
            registry: Some(registry),
            ..self
        }
    }

    pub fn registry(&self) -> Option<&str> {
        self.registry.as_deref()
    }

    pub fn with_single_package(self, package: String) -> Self {
        Self {
            single_package: Some(package),
            ..self
        }
    }

    pub fn with_repo_url(self, repo_url: RepoUrl) -> Self {
        Self {
            repo_url: Some(repo_url),
            ..self
        }
    }

    pub fn with_release_commits(self, release_commits: &str) -> anyhow::Result<Self> {
        let regex = Regex::new(release_commits).context("invalid release_commits regex pattern")?;

        Ok(Self {
            release_commits: Some(regex),
            ..self
        })
    }

    pub fn local_manifest_dir(&self) -> anyhow::Result<&Utf8Path> {
        self.local_manifest
            .parent()
            .context("wrong local manifest path")
    }

    pub fn local_manifest(&self) -> &Utf8Path {
        &self.local_manifest
    }

    pub fn registry_manifest(&self) -> Option<&Utf8Path> {
        self.registry_manifest.as_deref()
    }

    pub fn with_dependencies_update(self, dependencies_update: bool) -> Self {
        Self {
            dependencies_update,
            ..self
        }
    }

    pub fn should_update_dependencies(&self) -> bool {
        self.dependencies_update
    }

    pub fn with_allow_dirty(self, allow_dirty: bool) -> Self {
        Self {
            allow_dirty,
            ..self
        }
    }

    pub fn allow_dirty(&self) -> bool {
        self.allow_dirty
    }

    pub fn repo_url(&self) -> Option<&RepoUrl> {
        self.repo_url.as_ref()
    }

    pub fn packages_config(&self) -> &PackagesConfig {
        &self.packages_config
    }

    pub fn single_package(&self) -> Option<&str> {
        self.single_package.as_deref()
    }

    pub fn changelog_req(&self) -> &ChangelogRequest {
        &self.changelog_req
    }

    pub fn release_commits(&self) -> Option<&Regex> {
        self.release_commits.as_ref()
    }

    pub fn git_only(&self) -> Option<bool> {
        self.git_only
    }

    pub fn git_only_release_tag_prefix(&self) -> Option<&str> {
        self.git_only_release_tag_prefix.as_deref()
    }

    pub fn git_only_release_tag_suffix(&self) -> Option<&str> {
        self.git_only_release_tag_suffix.as_deref()
    }

    pub fn with_git_only(mut self, git_only: Option<bool>) -> Self {
        self.git_only = git_only;
        self
    }

    pub fn with_git_only_release_tag_prefix(mut self, prefix: Option<String>) -> Self {
        self.git_only_release_tag_prefix = prefix;
        self
    }

    pub fn with_git_only_release_tag_suffix(mut self, suffix: Option<String>) -> Self {
        self.git_only_release_tag_suffix = suffix;
        self
    }

    /// Determine if git_only mode should be used for a specific package.
    /// Package-level config overrides workspace-level config.
    pub fn should_use_git_only(&self, package_name: &str) -> bool {
        let pkg_config = self.get_package_config(package_name);

        // Package config takes precedence
        if let Some(git_only) = pkg_config.git_only() {
            return git_only;
        }

        // Fall back to workspace config
        self.git_only.unwrap_or(false)
    }

    /// Get the git_only release tag prefix for a specific package.
    /// Package-level config overrides workspace-level config.
    pub fn get_package_git_only_prefix(&self, package_name: &str) -> Option<String> {
        let pkg_config = self.get_package_config(package_name);

        // Package config takes precedence
        if let Some(prefix) = pkg_config.git_only_release_tag_prefix() {
            return Some(prefix.to_string());
        }

        // Fall back to workspace config
        self.git_only_release_tag_prefix.clone()
    }

    /// Get the git_only release tag suffix for a specific package.
    /// Package-level config overrides workspace-level config.
    pub fn get_package_git_only_suffix(&self, package_name: &str) -> Option<String> {
        let pkg_config = self.get_package_config(package_name);

        // Package config takes precedence
        if let Some(suffix) = pkg_config.git_only_release_tag_suffix() {
            return Some(suffix.to_string());
        }

        // Fall back to workspace config
        self.git_only_release_tag_suffix.clone()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PackagesConfig {
    /// Config for packages that don't have a specific configuration.
    default: UpdateConfig,
    /// Configurations that override `default`.
    /// The key is the package name.
    overrides: BTreeMap<String, PackageUpdateConfig>,
}

impl PackagesConfig {
    fn get(&self, package_name: &str) -> PackageUpdateConfig {
        self.overrides
            .get(package_name)
            .cloned()
            .unwrap_or(self.default.clone().into())
    }

    fn set_default(&mut self, config: UpdateConfig) {
        self.default = config;
    }

    fn set(&mut self, package_name: String, config: PackageUpdateConfig) {
        self.overrides.insert(package_name, config);
    }

    pub fn overridden_packages(&self) -> HashSet<&str> {
        self.overrides.keys().map(|s| s.as_str()).collect()
    }
}
