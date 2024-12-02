use crate::diff::Commit;
use crate::get_cargo_package_files;
use crate::{
    changelog_filler::{fill_commit, get_required_info},
    changelog_parser::{self, ChangelogRelease},
    copy_dir::copy_dir,
    diff::Diff,
    fs_utils::{strip_prefix, Utf8TempDir},
    is_readme_updated, local_readme_override, lock_compare,
    package_compare::are_packages_equal,
    package_path::{manifest_dir, PackagePath},
    published_packages::{self, PackagesCollection, PublishedPackage},
    repo_url::RepoUrl,
    semver_check::{self, SemverCheck},
    tmp_repo::TempRepo,
    toml_compare::are_toml_dependencies_updated,
    version::NextVersionFromDiff,
    ChangelogBuilder, PackagesToUpdate, PackagesUpdate, Project, Remote, CHANGELOG_FILENAME,
};
use crate::{GitBackend, GitClient};
use anyhow::Context;
use cargo_metadata::TargetKind;
use cargo_metadata::{
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
    Metadata, Package,
};
use cargo_utils::{canonical_local_manifest, upgrade_requirement, LocalManifest, CARGO_TOML};
use chrono::NaiveDate;
use git_cliff_core::contributor::RemoteContributor;
use git_cmd::{self, Repo};
use next_version::{NextVersion, VersionUpdater};
use rayon::prelude::{IntoParallelRefMutIterator, ParallelIterator};
use regex::Regex;
use std::path::PathBuf;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io,
    path::Path,
};
use toml_edit::TableLike;
use tracing::{debug, info, instrument, warn};

// Used to indicate that this is a dummy commit with no corresponding ID available.
// It should be at least 7 characters long to avoid a panic in git-cliff
// (Git-cliff assumes it's a valid commit ID).
pub(crate) const NO_COMMIT_ID: &str = "0000000";

#[derive(Debug)]
pub struct ReleaseMetadata {
    /// Template for the git tag created by release-plz.
    pub tag_name_template: Option<String>,
    /// Template for the git release name created by release-plz.
    pub release_name_template: Option<String>,
}

pub trait ReleaseMetadataBuilder {
    fn get_release_metadata(&self, package_name: &str) -> Option<ReleaseMetadata>;
}

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
    git: Option<GitBackend>,
}

#[derive(Debug, Clone, Default)]
struct PackagesConfig {
    /// Config for packages that don't have a specific configuration.
    default: UpdateConfig,
    /// Configurations that override `default`.
    /// The key is the package name.
    overrides: BTreeMap<String, PackageUpdateConfig>,
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
}

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
    /// Whether to use git tags instead of the Cargo registry to determine package versions.
    /// Default: `false`.
    pub git_only: bool,
    /// High-level toggle to process this package or ignore it.
    pub release: bool,
    /// - If `true`, feature commits will always bump the minor version, even in 0.x releases.
    /// - If `false` (default), feature commits will only bump the minor version starting with 1.x releases.
    pub features_always_increment_minor: bool,
    /// Template for the git tag created by release-plz.
    pub tag_name_template: Option<String>,
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

impl PackageUpdateConfig {
    pub fn semver_check(&self) -> bool {
        self.generic.semver_check
    }

    pub fn should_update_changelog(&self) -> bool {
        self.generic.changelog_update
    }

    pub fn git_only(&self) -> bool {
        self.generic.git_only
    }
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            semver_check: true,
            changelog_update: true,
            git_only: false,
            release: true,
            features_always_increment_minor: false,
            tag_name_template: None,
            changelog_path: None,
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

    pub fn version_updater(&self) -> VersionUpdater {
        VersionUpdater::default()
            .with_features_always_increment_minor(self.features_always_increment_minor)
    }

    pub fn with_git_only(self, git_only: bool) -> Self {
        Self { git_only, ..self }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChangelogRequest {
    /// When the new release is published. If unspecified, current date is used.
    pub release_date: Option<NaiveDate>,
    pub changelog_config: Option<git_cliff_core::config::Config>,
}

impl UpdateRequest {
    pub fn new(metadata: Metadata) -> anyhow::Result<Self> {
        let local_manifest = cargo_utils::workspace_manifest(&metadata);
        let local_manifest = canonical_local_manifest(local_manifest.as_ref())?;
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
                    .join(CHANGELOG_FILENAME)
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
            local_manifest: canonical_local_manifest(local_manifest.as_ref())?,
            ..self
        })
    }

    pub fn with_git_client(self, git: GitBackend) -> Self {
        Self {
            git: Some(git),
            ..self
        }
    }

    pub fn with_registry_manifest_path(self, registry_manifest: &Utf8Path) -> io::Result<Self> {
        let registry_manifest = Utf8Path::canonicalize_utf8(registry_manifest)?;
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

    pub fn repo_url(&self) -> Option<&RepoUrl> {
        self.repo_url.as_ref()
    }
}

impl ReleaseMetadataBuilder for UpdateRequest {
    fn get_release_metadata(&self, package_name: &str) -> Option<ReleaseMetadata> {
        let config = self.get_package_config(package_name);
        config.generic.release.then(|| ReleaseMetadata {
            tag_name_template: config.generic.tag_name_template.clone(),
            release_name_template: None,
        })
    }
}

/// Determine next version of packages
#[instrument(skip_all)]
pub async fn next_versions(input: &UpdateRequest) -> anyhow::Result<(PackagesUpdate, TempRepo)> {
    let overrides = input.packages_config.overrides.keys().cloned().collect();
    let local_project = Project::new(
        &input.local_manifest,
        input.single_package.as_deref(),
        &overrides,
        &input.metadata,
        input,
    )?;
    let updater = Updater {
        project: &local_project,
        req: input,
    };

    let repository = local_project
        .get_repo()
        .context("failed to determine local project repository")?;
    if !input.allow_dirty {
        repository.repo.is_clean()?;
    }

    // Retrieve the latest published version of the packages.
    // Release-plz will compare the registry packages with the local packages,
    // to determine the new commits.
    let publishable_packages = local_project.publishable_packages();

    let registry_published_packages = publishable_packages
        .iter()
        .copied()
        .filter(|p| !input.packages_config.get(&p.name).git_only());

    let git_only_published_packages = publishable_packages
        .iter()
        .copied()
        .filter(|p| input.packages_config.get(&p.name).git_only());

    let registry_packages = published_packages::get_latest_packages(
        &local_project,
        &repository,
        registry_published_packages,
        git_only_published_packages,
        input.registry_manifest(),
        input.registry.as_deref(),
    )?;

    let packages_to_update = updater
        .packages_to_update(&registry_packages, &repository.repo, input.local_manifest())
        .await?;
    Ok((packages_to_update, repository))
}

pub fn root_repo_path(local_manifest: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let manifest_dir = manifest_dir(local_manifest)?;
    root_repo_path_from_manifest_dir(manifest_dir)
}

pub fn root_repo_path_from_manifest_dir(manifest_dir: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let root = git_cmd::git_in_dir(manifest_dir, &["rev-parse", "--show-toplevel"])?;
    Ok(Utf8PathBuf::from(root))
}

pub fn new_manifest_dir_path(
    old_project_root: &Utf8Path,
    old_manifest_dir: &Utf8Path,
    new_project_root: &Utf8Path,
) -> anyhow::Result<Utf8PathBuf> {
    let parent_root = old_project_root.parent().unwrap_or(old_project_root);
    let relative_manifest_dir = strip_prefix(old_manifest_dir, parent_root)
        .context("cannot strip prefix for manifest dir")?;
    Ok(new_project_root.join(relative_manifest_dir))
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub version: Version,
    pub changelog: Option<String>,
    pub semver_check: SemverCheck,
}

impl UpdateResult {
    pub fn last_changes(&self) -> anyhow::Result<Option<ChangelogRelease>> {
        match &self.changelog {
            Some(c) => changelog_parser::last_release_from_str(c),
            None => Ok(None),
        }
    }
}

pub struct Updater<'a> {
    pub project: &'a Project,
    pub req: &'a UpdateRequest,
}

impl Updater<'_> {
    #[instrument(skip_all)]
    async fn packages_to_update(
        &self,
        registry_packages: &PackagesCollection,
        repository: &Repo,
        local_manifest_path: &Utf8Path,
    ) -> anyhow::Result<PackagesUpdate> {
        debug!("calculating local packages");

        let packages_diffs = self
            .get_packages_diffs(registry_packages, repository)
            .await?;
        let version_groups = self.get_version_groups(&packages_diffs);
        debug!("version groups: {:?}", version_groups);

        let mut packages_to_check_for_deps: Vec<&Package> = vec![];
        let mut packages_to_update = PackagesUpdate::default();

        let workspace_version_pkgs: HashSet<String> = packages_diffs
            .iter()
            .filter(|(p, _)| {
                let local_manifest_path = p.package_path().unwrap().join(CARGO_TOML);
                let local_manifest = LocalManifest::try_new(&local_manifest_path).unwrap();
                local_manifest.version_is_inherited()
            })
            .map(|(p, _)| p.name.clone())
            .collect();

        let new_workspace_version = self.new_workspace_version(
            local_manifest_path,
            &packages_diffs,
            &workspace_version_pkgs,
        )?;
        if let Some(new_workspace_version) = &new_workspace_version {
            packages_to_update.with_workspace_version(new_workspace_version.clone());
        }

        let mut old_changelogs = OldChangelogs::new();
        for (p, diff) in packages_diffs {
            if let Some(ref release_commits_regex) = self.req.release_commits {
                if !diff.any_commit_matches(release_commits_regex) {
                    continue;
                };
            }
            let next_version = self.get_next_version(
                new_workspace_version.as_ref(),
                p,
                &workspace_version_pkgs,
                &version_groups,
                &diff,
            )?;
            debug!(
                "package: {}, diff: {diff:?}, next_version: {next_version}",
                p.name,
            );
            let current_version = p.version.clone();
            if next_version != current_version || !diff.registry_package_exists {
                info!(
                    "{}: next version is {next_version}{}",
                    p.name,
                    diff.semver_check.outcome_str()
                );
                let update_result = self.calculate_update_result(
                    diff.commits,
                    next_version,
                    p,
                    diff.semver_check,
                    &mut old_changelogs,
                )?;
                packages_to_update
                    .updates_mut()
                    .push((p.clone(), update_result));
            } else if diff.is_version_published {
                // We need to update this package only if one of its dependencies has changed.
                packages_to_check_for_deps.push(p);
            }
        }

        let changed_packages: Vec<(&Package, &Version)> = packages_to_update
            .updates()
            .iter()
            .map(|(p, u)| (p, &u.version))
            .collect();
        let dependent_packages =
            self.dependent_packages_update(&packages_to_check_for_deps, &changed_packages)?;
        packages_to_update.updates_mut().extend(dependent_packages);
        Ok(packages_to_update)
    }

    /// Get the highest next version of all packages for each version group.
    fn get_version_groups(&self, packages_diffs: &[(&Package, Diff)]) -> HashMap<String, Version> {
        let mut version_groups: HashMap<String, Version> = HashMap::new();

        for (pkg, diff) in packages_diffs {
            let pkg_config = self.req.get_package_config(&pkg.name);
            let version_updater = pkg_config.generic.version_updater();
            if let Some(version_group) = pkg_config.version_group {
                let next_pkg_ver = pkg.version.next_from_diff(diff, version_updater);
                match version_groups.entry(version_group.clone()) {
                    std::collections::hash_map::Entry::Occupied(v) => {
                        // maximum version of the group until now
                        let max = v.get();
                        if max < &next_pkg_ver {
                            version_groups.insert(version_group, next_pkg_ver);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(_) => {
                        version_groups.insert(version_group, next_pkg_ver);
                    }
                }
            }
        }

        version_groups
    }

    fn new_workspace_version(
        &self,
        local_manifest_path: &Utf8Path,
        packages_diffs: &[(&Package, Diff)],
        workspace_version_pkgs: &HashSet<String>,
    ) -> anyhow::Result<Option<Version>> {
        let workspace_version = {
            let local_manifest = LocalManifest::try_new(local_manifest_path)?;
            local_manifest.get_workspace_version()
        };
        let new_workspace_version = workspace_version_pkgs
            .iter()
            .filter_map(|workspace_package| {
                for (p, diff) in packages_diffs {
                    if workspace_package == &p.name {
                        let pkg_config = self.req.get_package_config(&p.name);
                        let version_updater = pkg_config.generic.version_updater();
                        let next = p.version.next_from_diff(diff, version_updater);
                        if let Some(workspace_version) = &workspace_version {
                            if &next >= workspace_version {
                                return Some(next);
                            }
                        }
                    }
                }
                None
            })
            .max();
        Ok(new_workspace_version)
    }

    async fn get_packages_diffs(
        &self,
        registry_packages: &PackagesCollection,
        repository: &Repo,
    ) -> anyhow::Result<Vec<(&Package, Diff)>> {
        // Store diff for each package. This operation is not thread safe, so we do it in one
        // package at a time.
        let packages_diffs_res: anyhow::Result<Vec<(&Package, Diff)>> = self
            .project
            .publishable_packages()
            .iter()
            .map(|&p| {
                let diff = self
                    .get_diff(p, registry_packages, repository)
                    .context("failed to retrieve difference between packages")?;
                Ok((p, diff))
            })
            .collect();

        let mut packages_diffs = self.fill_commits(&packages_diffs_res?, repository).await?;
        let packages_commits: HashMap<String, Vec<Commit>> = packages_diffs
            .iter()
            .map(|(p, d)| (p.name.clone(), d.commits.clone()))
            .collect();

        let semver_check_result: anyhow::Result<()> =
            packages_diffs.par_iter_mut().try_for_each(|(p, diff)| {
                let registry_package = registry_packages.get_package(&p.name);
                if let Some(registry_package) = registry_package {
                    let package_path = get_package_path(p, repository, self.project.root())
                        .context("can't retrieve package path")?;
                    let package_config = self.req.get_package_config(&p.name);
                    for pkg_to_include in &package_config.changelog_include {
                        if let Some(commits) = packages_commits.get(pkg_to_include) {
                            diff.add_commits(commits);
                        }
                    }
                    if should_check_semver(p, package_config.semver_check())
                        && diff.should_update_version()
                    {
                        let registry_package_path = registry_package
                            .package_path()
                            .context("can't retrieve registry package path")?;
                        let semver_check =
                            semver_check::run_semver_check(&package_path, registry_package_path)
                                .context("error while running cargo-semver-checks")?;
                        diff.set_semver_check(semver_check);
                    }
                }
                Ok(())
            });
        semver_check_result?;

        Ok(packages_diffs)
    }

    async fn fill_commits<'a>(
        &self,
        packages_diffs: &[(&'a Package, Diff)],
        repository: &Repo,
    ) -> anyhow::Result<Vec<(&'a Package, Diff)>> {
        let git_client = self.req.git_client()?;
        let changelog_request: &ChangelogRequest = &self.req.changelog_req;
        let mut all_commits: HashMap<String, &Commit> = HashMap::new();
        let mut packages_diffs = packages_diffs.to_owned();
        if let Some(changelog_config) = changelog_request.changelog_config.as_ref() {
            let required_info = get_required_info(&changelog_config.changelog);
            for (_package, diff) in &mut packages_diffs {
                for commit in &mut diff.commits {
                    fill_commit(
                        commit,
                        &required_info,
                        repository,
                        &mut all_commits,
                        git_client.as_ref(),
                    )
                    .await?;
                }
            }
        }
        Ok(packages_diffs)
    }

    /// Return the update to apply to the packages that depend on the `changed_packages`.
    ///
    /// ## Args
    ///
    /// - `packages_to_check_for_deps`: The packages that might need to be updated.
    ///   We update them if they depend on any of the `changed_packages`.
    ///   If they don't depend on any of the `changed_packages`, they are not updated
    ///   because they don't contain any new commits.
    /// - `changed_packages`: The packages that have changed (i.e. contains commits).
    fn dependent_packages_update(
        &self,
        packages_to_check_for_deps: &[&Package],
        changed_packages: &[(&Package, &Version)],
    ) -> anyhow::Result<PackagesToUpdate> {
        let workspace_manifest = LocalManifest::try_new(self.req.local_manifest())?;
        let workspace_dependencies = workspace_manifest.get_workspace_dependency_table();

        let mut old_changelogs = OldChangelogs::new();
        let workspace_dir = manifest_dir(self.req.local_manifest())?;
        let packages_to_update = packages_to_check_for_deps
            .iter()
            .filter_map(|p| {
                p.dependencies_to_update(changed_packages, workspace_dependencies, workspace_dir)
                    .ok()
                    .filter(|deps| !deps.is_empty())
                    .map(|deps| (p, deps))
            })
            .map(|(&p, deps)| self.calculate_package_update_result(&deps, p, &mut old_changelogs))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(packages_to_update)
    }

    fn calculate_package_update_result(
        &self,
        deps: &[&Package],
        p: &Package,
        old_changelogs: &mut OldChangelogs,
    ) -> anyhow::Result<(Package, UpdateResult)> {
        let deps: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        let commits = {
            let change = format!(
                "chore: updated the following local packages: {}",
                deps.join(", ")
            );
            vec![Commit::new(NO_COMMIT_ID.to_string(), change)]
        };
        let next_version = { p.version.increment_patch() };
        info!(
            "{}: dependencies changed. Next version is {next_version}",
            p.name
        );
        let update_result = self.calculate_update_result(
            commits,
            next_version,
            p,
            SemverCheck::Skipped,
            old_changelogs,
        )?;
        Ok((p.clone(), update_result))
    }

    fn calculate_update_result(
        &self,
        commits: Vec<Commit>,
        next_version: Version,
        p: &Package,
        semver_check: SemverCheck,
        old_changelogs: &mut OldChangelogs,
    ) -> Result<UpdateResult, anyhow::Error> {
        let changelog_path = self.req.changelog_path(p);
        let old_changelog: Option<String> = old_changelogs.get_or_read(&changelog_path);
        let update_result = self.update_result(
            commits,
            next_version,
            p,
            semver_check,
            old_changelog.as_deref(),
        )?;
        if let Some(changelog) = &update_result.changelog {
            old_changelogs.insert(changelog_path, changelog.clone());
        }
        Ok(update_result)
    }

    /// This function needs `old_changelog` so that you can have changes of different
    /// packages in the same changelog.
    fn update_result(
        &self,
        commits: Vec<Commit>,
        version: Version,
        package: &Package,
        semver_check: SemverCheck,
        old_changelog: Option<&str>,
    ) -> anyhow::Result<UpdateResult> {
        let repo_url = self.req.repo_url.as_ref();
        let release_link = {
            let prev_tag = self
                .project
                .git_tag(&package.name, &package.version.to_string());
            let next_tag = self.project.git_tag(&package.name, &version.to_string());
            repo_url.map(|r| r.git_release_link(&prev_tag, &next_tag))
        };

        let pr_link = repo_url.map(|r| r.git_pr_link());

        lazy_static::lazy_static! {
            // match PR/issue numbers, e.g. `#123`
            static ref PR_RE: Regex = Regex::new("#(\\d+)").unwrap();
        }
        let changelog = {
            let cfg = self.req.get_package_config(package.name.as_str());
            let changelog_req = cfg
                .should_update_changelog()
                .then_some(self.req.changelog_req.clone());
            let commits: Vec<Commit> = commits
                .into_iter()
                // If not conventional commit, only consider the first line of the commit message.
                .filter_map(|c| {
                    if c.is_conventional() {
                        Some(c)
                    } else {
                        c.message.lines().next().map(|line| Commit {
                            message: line.to_string(),
                            ..c
                        })
                    }
                })
                // replace #123 with [#123](https://link_to_pr).
                // If the number refers to an issue, GitHub redirects the PR link to the issue link.
                .map(|c| {
                    if let Some(pr_link) = &pr_link {
                        let result = PR_RE.replace_all(&c.message, format!("[#$1]({pr_link}/$1)"));
                        Commit {
                            message: result.to_string(),
                            ..c
                        }
                    } else {
                        c
                    }
                })
                .collect();
            changelog_req
                .map(|r| {
                    get_changelog(
                        &commits,
                        &version,
                        Some(r),
                        old_changelog,
                        repo_url,
                        release_link.as_deref(),
                        package,
                    )
                })
                .transpose()
        }?;

        Ok(UpdateResult {
            version,
            changelog,
            semver_check,
        })
    }

    /// This operation is not thread-safe, because we do `git checkout` on the repository.
    #[instrument(
        skip_all,
        fields(package = %package.name)
    )]
    fn get_diff(
        &self,
        package: &Package,
        registry_packages: &PackagesCollection,
        repository: &Repo,
    ) -> anyhow::Result<Diff> {
        let package_path = get_package_path(package, repository, self.project.root())
            .context("failed to determine package path")?;

        repository
            .checkout_head()
            .context("can't checkout head to calculate diff")?;
        let registry_package = registry_packages.get_published_package(&package.name);
        let mut diff = Diff::new(registry_package.is_some());
        let pathbufs_to_check = pathbufs_to_check(&package_path, package);
        let paths_to_check: Vec<&Path> = pathbufs_to_check.iter().map(|p| p.as_ref()).collect();
        if let Err(err) = repository.checkout_last_commit_at_paths(&paths_to_check) {
            if err
                .to_string()
                .contains("Your local changes to the following files would be overwritten")
            {
                return Err(err.context("The allow-dirty option can't be used in this case"));
            } else {
                info!("{}: there are no commits", package.name);
                return Ok(diff);
            }
        }

        let git_tag = self
            .project
            .git_tag(&package.name, &package.version.to_string());
        let tag_commit = repository.get_tag_commit(&git_tag);
        if tag_commit.is_some() {
            let registry_package = registry_package.with_context(|| format!("package `{}` not found in the registry, but the git tag {git_tag} exists. Consider running `cargo publish` manually to publish this package.", package.name))?;
            anyhow::ensure!(
                registry_package.package.version == package.version,
                "package `{}` has a different version ({}) with respect to the registry package ({}), but the git tag {git_tag} exists. Consider running `cargo publish` manually to publish the new version of this package.",
                package.name, package.version, registry_package.package.version
            );
        }
        self.get_package_diff(
            &package_path,
            package,
            registry_package,
            repository,
            tag_commit.as_deref(),
            &mut diff,
        )?;
        repository
            .checkout_head()
            .context("can't checkout to head after calculating diff")?;
        Ok(diff)
    }

    fn get_package_diff(
        &self,
        package_path: &Utf8Path,
        package: &Package,
        registry_package: Option<&PublishedPackage>,
        repository: &Repo,
        tag_commit: Option<&str>,
        diff: &mut Diff,
    ) -> anyhow::Result<()> {
        let pathbufs_to_check = pathbufs_to_check(package_path, package);
        let paths_to_check: Vec<&Path> = pathbufs_to_check.iter().map(|p| p.as_ref()).collect();
        loop {
            let current_commit_message = repository.current_commit_message()?;
            let current_commit_hash = repository.current_commit_hash()?;

            // Check if files changed in git commit belong to the current package.
            // This is required because a package can contain another package in a subdirectory.
            let are_changed_files_in_pkg = || {
                self.are_changed_files_in_package(package_path, repository, &current_commit_hash)
            };

            if let Some(registry_package) = registry_package {
                debug!(
                    "package {} found in cargo registry",
                    registry_package.package.name
                );
                let registry_package_path = registry_package.package.package_path()?;

                let are_packages_equal = self.check_package_equality(
                    repository,
                    package,
                    package_path,
                    registry_package_path,
                )?;
                if are_packages_equal
                    || is_commit_too_old(
                        repository,
                        tag_commit,
                        registry_package.published_at_sha1(),
                        &current_commit_hash,
                    )
                {
                    debug!("next version calculated starting from commits after `{current_commit_hash}`");
                    if diff.commits.is_empty() {
                        self.add_dependencies_update_if_any(
                            diff,
                            &registry_package.package,
                            package,
                            registry_package_path,
                        )?;
                    }
                    // The local package is identical to the registry one, which means that
                    // the package was published at this commit, so we will not count this commit
                    // as part of the release.
                    // We can process the next create.
                    break;
                } else if registry_package.package.version != package.version {
                    info!("{}: the local package has already a different version with respect to the registry package, so release-plz will not update it", package.name);
                    diff.set_version_unpublished();
                    break;
                } else if are_changed_files_in_pkg()? {
                    debug!("packages are different");
                    // At this point of the git history, the two packages are different,
                    // which means that this commit is not present in the published package.
                    diff.commits.push(Commit::new(
                        current_commit_hash,
                        current_commit_message.clone(),
                    ));
                }
            } else if are_changed_files_in_pkg()? {
                diff.commits.push(Commit::new(
                    current_commit_hash,
                    current_commit_message.clone(),
                ));
            }
            // Go back to the previous commit.
            // Keep in mind that the info contained in `package` might be outdated,
            // because commits could contain changes to Cargo.toml.
            if let Err(_err) = repository.checkout_previous_commit_at_paths(&paths_to_check) {
                debug!("there are no other commits");
                break;
            }
        }
        Ok(())
    }

    fn check_package_equality(
        &self,
        repository: &Repo,
        package: &Package,
        package_path: &Utf8Path,
        registry_package_path: &Utf8Path,
    ) -> anyhow::Result<bool> {
        if is_readme_updated(&package.name, package_path, registry_package_path)? {
            debug!("{}: README updated", package.name);
            return Ok(false);
        }
        // We run `cargo package` when comparing packages, which can edit files, such as `Cargo.lock`.
        // Store its path so it can be reverted after comparison.
        let cargo_lock_path = self
            .get_cargo_lock_path(repository)
            .context("failed to determine Cargo.lock path")?;
        let are_packages_equal = are_packages_equal(package_path, registry_package_path)
            .context("cannot compare packages")?;
        if let Some(cargo_lock_path) = cargo_lock_path.as_deref() {
            // Revert any changes to `Cargo.lock`
            repository
                .checkout(cargo_lock_path)
                .context("cannot revert changes introduced when comparing packages")?;
        }
        Ok(are_packages_equal)
    }

    fn add_dependencies_update_if_any(
        &self,
        diff: &mut Diff,
        registry_package: &Package,
        package: &Package,
        registry_package_path: &Utf8Path,
    ) -> anyhow::Result<()> {
        let are_toml_dependencies_updated =
            || are_toml_dependencies_updated(&registry_package.dependencies, &package.dependencies);
        let are_lock_dependencies_updated = || {
            lock_compare::are_lock_dependencies_updated(
                &self.project.cargo_lock_path(),
                registry_package_path,
            )
            .context("Can't check if Cargo.lock dependencies are up to date")
        };
        if are_toml_dependencies_updated() {
            diff.commits.push(Commit::new(
                NO_COMMIT_ID.to_string(),
                "chore: update Cargo.toml dependencies".to_string(),
            ));
        } else if are_lock_dependencies_updated()? {
            diff.commits.push(Commit::new(
                NO_COMMIT_ID.to_string(),
                "chore: update Cargo.lock dependencies".to_string(),
            ));
        } else {
            info!("{}: already up to date", package.name);
        }
        Ok(())
    }

    fn get_cargo_lock_path(&self, repository: &Repo) -> anyhow::Result<Option<String>> {
        let project_cargo_lock = self.project.cargo_lock_path();
        let relative_lock_path = strip_prefix(&project_cargo_lock, self.project.root())?;
        let repository_cargo_lock = repository.directory().join(relative_lock_path);
        if repository_cargo_lock.exists() {
            Ok(Some(repository_cargo_lock.to_string()))
        } else {
            Ok(None)
        }
    }

    fn get_next_version(
        &self,
        new_workspace_version: Option<&Version>,
        p: &Package,
        workspace_version_pkgs: &HashSet<String>,
        version_groups: &HashMap<String, Version>,
        diff: &Diff,
    ) -> anyhow::Result<Version> {
        let pkg_config = self.req.get_package_config(&p.name);
        let next_version = match new_workspace_version {
            Some(max_workspace_version) if workspace_version_pkgs.contains(p.name.as_str()) => {
                debug!(
                    "next version of {} is workspace version: {max_workspace_version}",
                    p.name
                );
                max_workspace_version.clone()
            }
            _ => {
                if let Some(version_group) = pkg_config.version_group {
                    version_groups
                        .get(&version_group)
                        .with_context(|| {
                            format!("failed to retrieve version for version group {version_group}")
                        })?
                        .clone()
                } else {
                    let version_updater = pkg_config.generic.version_updater();
                    p.version.next_from_diff(diff, version_updater)
                }
            }
        };
        Ok(next_version)
    }

    /// `hash` is only used for logging purposes.
    fn are_changed_files_in_package(
        &self,
        package_path: &Utf8Path,
        repository: &Repo,
        hash: &str,
    ) -> anyhow::Result<bool> {
        // We run `cargo package` to get package files, which can edit files, such as `Cargo.lock`.
        // Store its path so it can be reverted after comparison.
        let cargo_lock_path = self
            .get_cargo_lock_path(repository)
            .context("failed to determine Cargo.lock path")?;
        let package_files_res = get_package_files(package_path, repository);
        if let Some(cargo_lock_path) = cargo_lock_path.as_deref() {
            // Revert any changes to `Cargo.lock`
            repository
                .checkout(cargo_lock_path)
                .context("cannot revert changes introduced when comparing packages")?;
        }
        let Ok(package_files) = package_files_res.inspect_err(|e| {
            debug!("failed to get package files at commit {hash}: {e:?}");
        }) else {
            // `cargo package` can fail if the package doesn't contain a Cargo.toml file yet.
            return Ok(true);
        };
        let Ok(changed_files) = repository.files_of_current_commit().inspect_err(|e| {
            warn!("failed to get changed files of commit {hash}: {e:?}");
        }) else {
            // Assume that this commit contains changes to the package.
            return Ok(true);
        };
        Ok(!package_files.is_disjoint(&changed_files))
    }
}

/// Get files that belong to the package.
/// The paths are relative to the git repo root.
fn get_package_files(
    package_path: &Utf8Path,
    repository: &Repo,
) -> anyhow::Result<HashSet<Utf8PathBuf>> {
    // Get relative path of the crate with respect to the repository because we need to compare
    // files with the git output.
    let crate_relative_path = package_path.strip_prefix(repository.directory())?;
    let sources = get_cargo_package_files(package_path)?
        .into_iter()
        // filter file generated by `cargo package` that isn't in git.
        .filter(|l| l != "Cargo.toml.orig" && l != ".cargo_vcs_info.json")
        .map(|l| {
            let is_crate_path_same_as_git_repo = crate_relative_path == "";
            if is_crate_path_same_as_git_repo {
                l
            } else {
                crate_relative_path.join(l)
            }
        })
        .collect();
    Ok(sources)
}

struct OldChangelogs {
    old_changelogs: HashMap<Utf8PathBuf, String>,
}

impl OldChangelogs {
    fn new() -> Self {
        Self {
            old_changelogs: HashMap::new(),
        }
    }

    fn get_or_read(&self, changelog_path: &Utf8PathBuf) -> Option<String> {
        self.old_changelogs
            .get(changelog_path)
            .cloned()
            .or(fs_err::read_to_string(changelog_path).ok())
    }

    fn insert(&mut self, changelog_path: Utf8PathBuf, changelog: String) {
        self.old_changelogs.insert(changelog_path, changelog);
    }
}

fn get_changelog(
    commits: &[Commit],
    next_version: &Version,
    changelog_req: Option<ChangelogRequest>,
    old_changelog: Option<&str>,
    repo_url: Option<&RepoUrl>,
    release_link: Option<&str>,
    package: &Package,
) -> anyhow::Result<String> {
    let commits: Vec<git_cliff_core::commit::Commit> =
        commits.iter().map(|c| c.to_cliff_commit()).collect();
    let mut changelog_builder = ChangelogBuilder::new(
        commits.clone(),
        next_version.to_string(),
        package.name.clone(),
    );
    if let Some(changelog_req) = changelog_req {
        if let Some(release_date) = changelog_req.release_date {
            changelog_builder = changelog_builder.with_release_date(release_date);
        }
        if let Some(config) = changelog_req.changelog_config {
            changelog_builder = changelog_builder.with_config(config);
        }
        if let Some(link) = release_link {
            changelog_builder = changelog_builder.with_release_link(link);
        }
        if let Some(repo_url) = repo_url {
            let remote = Remote {
                owner: repo_url.owner.clone(),
                repo: repo_url.name.clone(),
                link: repo_url.full_host(),
                contributors: get_contributors(&commits),
            };
            changelog_builder = changelog_builder.with_remote(remote);
        }
        let is_package_published = next_version != &package.version;

        let last_version = old_changelog.and_then(|old_changelog| {
            changelog_parser::last_version_from_str(old_changelog)
                .ok()
                .flatten()
        });
        if is_package_published {
            let last_version = last_version.unwrap_or(package.version.to_string());
            changelog_builder = changelog_builder.with_previous_version(last_version);
        } else if let Some(last_version) = last_version {
            if let Some(old_changelog) = old_changelog {
                if last_version == next_version.to_string() {
                    // If the next version is the same as the last version of the changelog,
                    // don't update the changelog (returning the old one).
                    // This can happen when no version of the package was published,
                    // but the changelog already contains the changes of the initial version
                    // of the package (e.g. because a release PR was merged).
                    return Ok(old_changelog.to_string());
                }
            }
        }
    }
    let new_changelog = changelog_builder.build();
    let changelog = match old_changelog {
        Some(old_changelog) => new_changelog.prepend(old_changelog)?,
        None => new_changelog.generate()?, // Old changelog doesn't exist.
    };
    Ok(changelog)
}

fn get_contributors(commits: &[git_cliff_core::commit::Commit]) -> Vec<RemoteContributor> {
    let mut unique_contributors = HashSet::new();
    commits
        .iter()
        .filter_map(|c| c.remote.clone())
        // Filter out duplicate contributors.
        // `insert` returns false if the contributor is already in the set.
        .filter(|remote| unique_contributors.insert(remote.username.clone()))
        .collect()
}

fn get_package_path(
    package: &Package,
    repository: &Repo,
    project_root: &Utf8Path,
) -> anyhow::Result<Utf8PathBuf> {
    let package_path = package.package_path()?;
    get_repo_path(package_path, repository, project_root)
}

fn get_repo_path(
    old_path: &Utf8Path,
    repository: &Repo,
    project_root: &Utf8Path,
) -> anyhow::Result<Utf8PathBuf> {
    let relative_path =
        strip_prefix(old_path, project_root).context("error while retrieving package_path")?;
    let result_path = repository.directory().join(relative_path);

    Ok(result_path)
}

/// Check if commit belongs to a previous version of the package.
/// `tag_commit` is the commit hash of the tag of the previous version.
/// `published_at_commit` is the commit hash where `cargo publish` ran.
fn is_commit_too_old(
    repository: &Repo,
    tag_commit: Option<&str>,
    published_at_commit: Option<&str>,
    current_commit_hash: &str,
) -> bool {
    if let Some(tag_commit) = tag_commit.as_ref() {
        if repository.is_ancestor(current_commit_hash, tag_commit) {
            debug!("stopping looking at git history because the current commit ({}) is an ancestor of the commit ({}) tagged with the previous version.", current_commit_hash, tag_commit);
            return true;
        }
    }

    if let Some(published_commit) = published_at_commit.as_ref() {
        if repository.is_ancestor(current_commit_hash, published_commit) {
            debug!("stopping looking at git history because the current commit ({}) is an ancestor of the commit ({}) where the previous version was published.", current_commit_hash, published_commit);
            return true;
        }
    }
    false
}

fn pathbufs_to_check(package_path: &Utf8Path, package: &Package) -> Vec<Utf8PathBuf> {
    let mut paths = vec![package_path.to_path_buf()];
    if let Some(readme_path) = local_readme_override(package, package_path) {
        paths.push(readme_path);
    }
    paths
}

/// Check if release-plz should check the semver compatibility of the package.
/// - `run_semver_check` is true if the user wants to run the semver check.
fn should_check_semver(package: &Package, run_semver_check: bool) -> bool {
    let is_cargo_semver_checks_installed = semver_check::is_cargo_semver_checks_installed;
    run_semver_check && is_library(package) && is_cargo_semver_checks_installed()
}

pub fn workspace_packages(metadata: &Metadata) -> anyhow::Result<Vec<Package>> {
    cargo_utils::workspace_members(metadata).map(|members| members.collect())
}

pub fn publishable_packages_from_manifest(
    manifest: impl AsRef<Utf8Path>,
) -> anyhow::Result<Vec<Package>> {
    let metadata = cargo_utils::get_manifest_metadata(manifest.as_ref())?;
    cargo_utils::workspace_members(&metadata)
        .map(|members| members.filter(|p| p.is_publishable()).collect())
}

pub trait Publishable {
    fn is_publishable(&self) -> bool;
}

impl Publishable for Package {
    /// Return true if the package can be published to at least one register (e.g. crates.io).
    fn is_publishable(&self) -> bool {
        if let Some(publish) = &self.publish {
            // `publish.is_empty()` is:
            // - true: when `publish` in Cargo.toml is `[]` or `false`.
            // - false: when the package can be published only to certain registries.
            //          E.g. when `publish` in Cargo.toml is `["my-reg"]` or `true`.
            !publish.is_empty()
        } else {
            // If it's not an example, the package can be published anywhere
            !is_example_package(self)
        }
    }
}

fn is_example_package(package: &Package) -> bool {
    package
        .targets
        .iter()
        .all(|t| t.kind == [TargetKind::Example])
}

fn is_library(package: &Package) -> bool {
    package
        .targets
        .iter()
        .any(|t| t.kind.contains(&TargetKind::Lib))
}

pub fn copy_to_temp_dir(target: &Utf8Path) -> anyhow::Result<Utf8TempDir> {
    let tmp_dir = Utf8TempDir::new().context("cannot create temporary directory")?;
    copy_dir(target, tmp_dir.path())
        .with_context(|| format!("cannot copy directory {target:?} to {tmp_dir:?}"))?;
    Ok(tmp_dir)
}

trait PackageDependencies {
    /// Returns the `updated_packages` which should be updated in the dependencies of the package.
    fn dependencies_to_update<'a>(
        &self,
        updated_packages: &'a [(&Package, &Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
    ) -> anyhow::Result<Vec<&'a Package>>;
}

impl PackageDependencies for Package {
    fn dependencies_to_update<'a>(
        &self,
        updated_packages: &'a [(&Package, &Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
    ) -> anyhow::Result<Vec<&'a Package>> {
        // Look into the toml manifest because `cargo_metadata` doesn't distinguish between
        // empty `version` in Cargo.toml and `version = "*"`
        let package_manifest = LocalManifest::try_new(&self.manifest_path)?;
        let package_dir = manifest_dir(&package_manifest.path)?.to_owned();

        let mut deps_to_update: Vec<&Package> = vec![];
        for (p, next_ver) in updated_packages {
            let canonical_path = p.canonical_path()?;
            // Find the dependencies that have the same path as the updated package.
            let matching_deps = package_manifest
                .get_dependency_tables()
                .flat_map(|t| {
                    t.iter().filter_map(|(name, d)| {
                        d.as_table_like().map(|d| {
                            match workspace_dependencies {
                                Some(workspace_dependencies) if is_workspace_dependency(d) => {
                                    // The dependency of the package Cargo.toml is inherited from the workspace,
                                    // so we find the dependency of the workspace and use it instead.
                                    let dep = workspace_dependencies
                                        .iter()
                                        .find(|(n, _)| n == &name)
                                        .and_then(|(_, d)| d.as_table_like())
                                        .unwrap_or(d);
                                    // Return also the path of the Cargo.toml so that we can resolve the
                                    // relative path of the dependency later.
                                    (workspace_dir, dep)
                                }
                                _ => (package_dir.as_path(), d),
                            }
                        })
                    })
                })
                // Exclude path dependencies without `version`.
                .filter(|(_toml_base_path, d)| d.contains_key("version"))
                .filter(|(toml_base_path, d)| {
                    is_dependency_referred_to_package(*d, toml_base_path, &canonical_path)
                })
                .map(|(_, dep)| dep);

            for dep in matching_deps {
                if should_update_dependency(dep, next_ver)? {
                    deps_to_update.push(p);
                }
            }
        }

        Ok(deps_to_update)
    }
}

/// Check if the dependency is in the form of `dep_name.workspace = true`.
fn is_workspace_dependency(d: &dyn TableLike) -> bool {
    d.get("workspace")
        .is_some_and(|w| w.as_bool() == Some(true))
        && !d.contains_key("version")
        && !d.contains_key("path")
}

/// Check if `dependency` (contained in the Cargo.toml at `dependency_package_dir`) refers
/// to the package at `package_dir`.
/// I.e. if the absolute path of the dependency is the same as the absolute path of the package.
pub(crate) fn is_dependency_referred_to_package(
    dependency: &dyn TableLike,
    package_dir: &Utf8Path,
    dependency_package_dir: &Utf8Path,
) -> bool {
    canonicalized_path(dependency, package_dir)
        .is_some_and(|dep_path| dep_path == dependency_package_dir)
}

/// Dependencies are expressed as relative paths in the Cargo.toml file.
/// This function returns the absolute path of the dependency.
///
/// ## Args
///
/// - `package_dir`: directory containing the Cargo.toml where the dependency is listed
/// - `dependency`: entry of the Cargo.toml
fn canonicalized_path(dependency: &dyn TableLike, package_dir: &Utf8Path) -> Option<PathBuf> {
    dependency
        .get("path")
        .and_then(|i| i.as_str())
        .and_then(|relpath| dunce::canonicalize(package_dir.join(relpath)).ok())
}

fn should_update_dependency(dep: &dyn TableLike, next_ver: &Version) -> anyhow::Result<bool> {
    let old_req = dep
        .get("version")
        .expect("filter ensures this")
        .as_str()
        .unwrap_or("*");
    let should_update_dep = upgrade_requirement(old_req, next_ver)?.is_some();
    Ok(should_update_dep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_version_is_not_added_to_changelog() {
        let commits = vec![
            Commit::new(crate::NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
            Commit::new(crate::NO_COMMIT_ID.to_string(), "simple update".to_string()),
        ];

        let next_version = Version::new(1, 1, 0);
        let changelog_req = ChangelogRequest::default();

        let old = r#"## [1.1.0] - 1970-01-01

### fix bugs
- my awesomefix

### other
- complex update
"#;
        let new = get_changelog(
            &commits,
            &next_version,
            Some(changelog_req),
            Some(old),
            None,
            None,
            &fake_package::FakePackage::new("my_package").into(),
        )
        .unwrap();
        assert_eq!(old, new);
    }
}
