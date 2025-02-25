use crate::update_request::UpdateRequest;
use crate::updater::Updater;
use crate::{
    PackagesUpdate, Project,
    changelog_parser::{self, ChangelogRelease},
    copy_dir::copy_dir,
    fs_utils::{Utf8TempDir, strip_prefix},
    package_path::manifest_dir,
    registry_packages::{self},
    semver_check::SemverCheck,
    tmp_repo::TempRepo,
};
use anyhow::Context;
use cargo_metadata::TargetKind;
use cargo_metadata::{
    Metadata, Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use chrono::NaiveDate;
use git_cmd::{self};
use std::path::PathBuf;
use toml_edit::TableLike;
use tracing::instrument;

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

#[derive(Debug, Clone, Default)]
pub struct ChangelogRequest {
    /// When the new release is published. If unspecified, current date is used.
    pub release_date: Option<NaiveDate>,
    pub changelog_config: Option<git_cliff_core::config::Config>,
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
    let overrides = input.packages_config().overridden_packages();
    let local_project = Project::new(
        input.local_manifest(),
        input.single_package(),
        &overrides,
        input.cargo_metadata(),
        input,
    )?;
    let updater = Updater {
        project: &local_project,
        req: input,
    };
    // Retrieve the latest published version of the packages.
    // Release-plz will compare the registry packages with the local packages,
    // to determine the new commits.
    let registry_packages = registry_packages::get_registry_packages(
        input.registry_manifest(),
        &local_project.publishable_packages(),
        input.registry(),
    )?;

    let repository = local_project
        .get_repo()
        .context("failed to determine local project repository")?;

    let repo_is_clean_result = repository.repo.is_clean();
    if !input.allow_dirty() {
        repo_is_clean_result?;
    } else if repo_is_clean_result.is_err() {
        // Stash uncommitted changes so we can freely check out other commits.
        // This function is ran inside a temporary repository, so this has no
        // effects on the original repository of the user.
        repository.repo.git(&[
            "stash",
            "push",
            "--include-untracked",
            "-m",
            "uncommitted changes stashed by release-plz",
        ])?;
    }
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
    /// Next version of the package.
    pub version: Version,
    /// New changelog.
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

pub fn copy_to_temp_dir(target: &Utf8Path) -> anyhow::Result<Utf8TempDir> {
    let tmp_dir = Utf8TempDir::new().context("cannot create temporary directory")?;
    copy_dir(target, tmp_dir.path())
        .with_context(|| format!("cannot copy directory {target:?} to {tmp_dir:?}"))?;
    Ok(tmp_dir)
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
