use crate::command::git::{CustomRepo, CustomWorkTree};
use crate::registry_packages::{PackagesCollection, RegistryPackage};
use crate::tmp_repo::TempRepo;
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
};
use anyhow::Context;
use cargo_metadata::{
    Metadata, Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_metadata::{MetadataCommand, TargetKind};
use chrono::NaiveDate;
use regex::Regex;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::PathBuf;
use toml_edit::TableLike;
use tracing::{debug, info, instrument, trace, warn};

// Used to indicate that this is a dummy commit with no corresponding ID available.
// It should be at least 7 characters long to avoid a panic in git-cliff
// (Git-cliff assumes it's a valid commit ID).
pub(crate) const NO_COMMIT_ID: &str = "0000000";

#[derive(Debug, Clone)]
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

// Build regex: ^{escaped_prefix}(\d+\.\d+\.\d+){escaped_suffix}$
// The semantic version is captured in group 1
fn get_release_regex(prefix: &str, suffix: &str) -> anyhow::Result<Regex> {
    let escaped_prefix = regex::escape(&prefix);
    let escaped_suffix = regex::escape(&suffix);
    let release_regex_str = format!(r"^{}(\d+\.\d+\.\d+){}$", escaped_prefix, escaped_suffix);
    Regex::new(&release_regex_str).context("failed to build release tag regex")
}

// create a temporary worktree and its associated repo
//
// if using the CLI, working in a worktree is the same as working in a repo, but in git2 they are
// considered different objects with different methods so we return both. The drop order for these
// doesn't actually matter, because the repo will become invalid when the worktree drops. But we
// typically want to drop the repo first jsut to avoid the possibility of someone using an invalid
// repo.
fn get_temp_worktree_and_repo(
    original_repo: &mut CustomRepo,
    package_name: &str,
) -> anyhow::Result<(CustomRepo, CustomWorkTree)> {
    // Clean up any existing worktree with this name
    original_repo
        .cleanup_worktree_if_exists(package_name)
        .context("cleanup existing worktree")?;

    // make a worktree for the package
    let worktree = original_repo
        .temp_worktree(Some(package_name), package_name)
        .context("build worktree for package")?;

    // create repo at new worktree
    // git2 worktrees don't really contain any functionality, so we have to create a repo
    // using that path
    let repo = CustomRepo::open(worktree.path()).context("open repo for package")?;

    Ok((repo, worktree))
}

// run cargo publish within a worktree
fn run_cargo_publish(worktree: &CustomWorkTree) -> anyhow::Result<()> {
    // run cargo package so we get the proper format
    let output = std::process::Command::new("cargo")
        .args(["package", "--allow-dirty"])
        .current_dir(worktree.path())
        .output()
        .context("run cargo package in worktree")?;

    if !output.status.success() {
        anyhow::bail!(
            "cargo package failed: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn get_cargo_package(worktree: &CustomWorkTree, package_name: &str) -> anyhow::Result<Package> {
    // create the package / registry package
    let rust_package = MetadataCommand::new()
        .manifest_path(format!(
            "{}/Cargo.toml",
            worktree
                .path()
                .to_str()
                .context("convert worktree path to str")?
        ))
        .exec()
        .context("get cargo metadata")?;

    let package_details = rust_package
        .packages
        .iter()
        .find(|x| x.name == package_name)
        .ok_or(anyhow::anyhow!("Failed to find package {:?}", package_name))?;

    let new_path = format!(
        "{}/target/package/{}-{}",
        worktree
            .path()
            .to_str()
            .context("convert worktree path to str")?,
        package_details.name,
        package_details.version
    );
    info!("package for {} is at {}", package_name, new_path);

    // create the package
    let single_package_meta = MetadataCommand::new()
        .manifest_path(format!("{}/Cargo.toml", new_path))
        .exec()
        .context("get cargo metadata")?;

    // get the package details
    let single_package = single_package_meta
        .workspace_packages()
        .into_iter()
        .find(|p| p.name == package_name)
        .ok_or(anyhow::Error::msg("Couldn't find the package"))?
        .clone();

    Ok(single_package)
}

/// Determine next version of packages
/// Any packages that will be updated will be returned, alongside whether we update the workspace
/// The temp repository is an isolated copy used for git operations
#[instrument(skip_all)]
pub async fn next_versions(input: &UpdateRequest) -> anyhow::Result<(PackagesUpdate, TempRepo)> {
    info!("determining next version");
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

    // Separate packages based on per-package git_only configuration
    let workspace_packages = input.cargo_metadata().workspace_packages();
    let mut git_only_packages = Vec::new();
    let mut registry_packages_list = Vec::new();

    for package in &workspace_packages {
        if input.should_use_git_only(&package.name) {
            git_only_packages.push(*package);
        } else {
            registry_packages_list.push(*package);
        }
    }

    // We'll collect all packages packages we'll be updating in a single map
    let mut all_packages: BTreeMap<String, RegistryPackage> = BTreeMap::new();

    // SAFETY: We need to prevent the worktrees from being dropped because their Drop
    // implementation cleans up the worktrees.
    //
    // See the note on the custom worktree Drop impl for more details
    let mut worktrees: Vec<CustomWorkTree> = Vec::new();

    // Process git_only packages
    if !git_only_packages.is_empty() {
        debug!(
            "Processing {} packages in git_only mode",
            git_only_packages.len()
        );

        // create the repo we'll be spinning worktrees from
        let mut unreleased_project_repo = CustomRepo::open(
            input
                .local_manifest_dir()
                .context("get local manifest dir")?,
        )
        .context("create unreleased repo for spinning worktrees")?;

        for package in git_only_packages {
            // enter a new span for each package, just for clarity and avoiding needing to pollute
            // all of our logs with the package name
            let ispan = tracing::info_span!("git_only_package");
            let _enter = ispan.enter();
            ispan.record("package_name", package.name.to_string());

            // get the release regex for this package
            let release_regex = get_release_regex(
                input
                    .get_package_git_only_prefix(&package.name)
                    .unwrap_or_default()
                    .as_str(),
                input
                    .get_package_git_only_suffix(&package.name)
                    .unwrap_or_default()
                    .as_str(),
            )
            .context("get release regex")?;
            info!(
                "looking for tags matching pattern: {}",
                release_regex.to_string()
            );

            // get the temporary worktree and repo that we run cargo package in
            let (mut repo, worktree) =
                get_temp_worktree_and_repo(&mut unreleased_project_repo, &package.name)
                    .context("get worktree and repo for package")?;

            let (release_tag, version) = match repo
                .get_release_tag(&release_regex, &package.name)
                .context("get release tag")?
            {
                Some((a, b)) => (a, b),
                None => {
                    warn!(
                        "no release tag matching pattern: {}",
                        release_regex.to_string()
                    );
                    continue;
                }
            };

            info!("using tag `{}` (version {})", release_tag, version);

            // get the commit associated with the release tag
            let release_commit = repo
                .get_tag_commit(&release_tag)
                .context("get release tag commit")?;

            // checkout that commit in the worktree
            repo.checkout_commit(&release_commit)
                .context("checkout release commit for package")?;

            // run cargo publish so we have our finalized package
            run_cargo_publish(&worktree).context("run cargo publish")?;

            // get the package
            let single_package = get_cargo_package(&worktree, &package.name)
                .context("get cargo package from worktree")?;

            // add it to the B Tree map
            all_packages.insert(
                single_package.name.to_string(),
                RegistryPackage::new(single_package, Some(release_commit)),
            );

            // SEE SAFETY NOTE ABOVE
            worktrees.push(worktree);
        }
    }

    // Process non-git_only packages (download from registry)
    if !registry_packages_list.is_empty() {
        debug!(
            "Processing {} packages from registry",
            registry_packages_list.len()
        );

        // Filter to only publishable packages for registry download
        let publishable_registry_packages: Vec<&Package> = registry_packages_list
            .into_iter()
            .filter(|p| {
                local_project
                    .publishable_packages()
                    .iter()
                    .any(|pub_pkg| pub_pkg.name == p.name)
            })
            .collect();

        if !publishable_registry_packages.is_empty() {
            // Retrieve the latest published version of the packages.
            // Release-plz will compare the registry packages with the local packages,
            // to determine the new commits.
            let registry_packages = registry_packages::get_registry_packages(
                input.registry_manifest(),
                &publishable_registry_packages,
                input.registry(),
            )?;

            // Merge registry packages into all_packages
            // We need to extract the packages from PackagesCollection
            for package_name in publishable_registry_packages.iter().map(|p| &p.name) {
                if let Some(reg_pkg) = registry_packages.get_registry_package(package_name) {
                    all_packages.insert(
                        package_name.to_string(),
                        RegistryPackage::new(
                            reg_pkg.package.clone(),
                            reg_pkg.published_at_sha1().map(|s| s.to_string()),
                        ),
                    );
                }
            }
        }
    }

    let release_packages = PackagesCollection::default().with_packages(all_packages);

    // Create a temporary isolated repository for git operations.
    // This ensures that git checkouts and other operations don't affect the user's working directory.
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
        .packages_to_update(&release_packages, &repository.repo, input.local_manifest())
        .await?;
    Ok((packages_to_update, repository))
}

pub async fn delete_existing_worktree(path: &Utf8Path) -> anyhow::Result<()> {
    // We still have to check if this worktree exists already. It shouldn't, but because NTP
    // could in theory step the clock back, we may as well check.
    let f = tokio::fs::File::open(&path).await;

    // doesn't exist, great
    if f.as_ref().is_err_and(|x| x.kind() == ErrorKind::NotFound) {
        debug!("{path} not found, nothing to delete");
    }
    // does exist, delete it
    else {
        match f?.metadata().await {
            Ok(m) if m.is_dir() => {
                warn!(
                    "{path} already exists, are you a time traveller? Just kidding, but we are deleting it for consistency"
                );
                tokio::fs::remove_dir_all(&path)
                    .await
                    .context("delete existing unreleased package")?;
            }

            // either file or symlink, i don't think we care though? we'll just remove it
            Ok(_) => {
                warn!("{path} already exists as a file, deleting it");
                tokio::fs::remove_file(&path)
                    .await
                    .context("delete existing unreleased package file")?;
            }

            // if its not found, then great!
            Err(e) => return Err(anyhow::anyhow!(e)),
        };
    }

    Ok(())
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
    pub new_changelog_entry: Option<String>,
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
        let res = if let Some(publish) = &self.publish {
            // `publish.is_empty()` is:
            // - true: when `publish` in Cargo.toml is `[]` or `false`.
            // - false: when the package can be published only to certain registries.
            //          E.g. when `publish` in Cargo.toml is `["my-reg"]` or `true`.
            !publish.is_empty()
        } else {
            // If it's not an example, the package can be published anywhere
            !is_example_package(self)
        };
        trace!("package {} is publishable: {res}", self.name);
        res
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
