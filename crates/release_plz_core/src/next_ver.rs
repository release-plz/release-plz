use crate::cargo::run_cargo;
use crate::command::git::{GitRepo, GitWorkTree};
use crate::registry_packages::{PackagesCollection, RegistryPackage};
use crate::release_regex;
use crate::tera::default_tag_name_template;
use crate::tmp_repo::TempRepo;
use crate::update_request::UpdateRequest;
use crate::updater::Updater;
use crate::{
    PackagesUpdate, Project,
    changelog_parser::{self, ChangelogRelease},
    copy_dir::copy_dir,
    fs_utils::{Utf8TempDir, strip_prefix, to_utf8_path},
    package_path::manifest_dir,
    registry_packages::{self},
    semver_check::SemverCheck,
};
use anyhow::Context;
use cargo_metadata::TargetKind;
use cargo_metadata::{
    Metadata, MetadataCommand, Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_utils::get_manifest_metadata;
use chrono::NaiveDate;
use std::collections::BTreeMap;
use std::path::PathBuf;
use toml_edit::TableLike;
use tracing::{debug, info, instrument, trace};

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

/// Create a temporary worktree and its associated repo.
///
/// If using the CLI, working in a worktree is the same as working in a repo, but in git2 they are
/// considered different objects with different methods so we return both. The drop order for these
/// doesn't actually matter, because the repo will become invalid when the worktree drops. But we
/// typically want to drop the repo first just to avoid the possibility of someone using an invalid
/// repo.
fn get_temp_worktree_and_repo(
    original_repo: &mut GitRepo,
    package_name: &str,
) -> anyhow::Result<(GitRepo, GitWorkTree)> {
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
    let repo = GitRepo::open(worktree.path()).context("open repo for package")?;

    Ok((repo, worktree))
}

/// Process a single `git_only` package: find its release tag, checkout that commit,
/// run `cargo package`, and return the package metadata.
///
/// Returns `None` if no release tag is found (package will be treated as initial release).
#[instrument(skip_all, fields(package_name = %package.name))]
fn process_git_only_package(
    package: &Package,
    unreleased_project_repo: &mut GitRepo,
    input: &UpdateRequest,
    is_multi_package: bool,
) -> anyhow::Result<Option<(RegistryPackage, GitWorkTree)>> {
    // Get the release tag template, falling back to default based on project structure
    let template = input
        .get_package_tag_name(&package.name)
        .unwrap_or_else(|| default_tag_name_template(is_multi_package));

    let release_regex =
        release_regex::get_release_regex(&template, &package.name).context("get release regex")?;
    debug!(
        "looking for tags matching pattern: {}",
        release_regex.to_string()
    );

    // Get the temporary worktree and repo that we run cargo package in
    let (mut repo, worktree) = get_temp_worktree_and_repo(unreleased_project_repo, &package.name)
        .context("get worktree and repo for package")?;

    let Some((release_tag, version)) = repo
        .get_release_tag(&release_regex, &package.name)
        .context("get release tag")?
    else {
        info!(
            "No release tag found matching pattern `{release_regex}`. \
             Package {} will be treated as initial release.",
            package.name
        );
        return Ok(None);
    };

    info!(
        "Latest release of package {}: tag `{release_tag}` (version {version})",
        package.name
    );

    // Get the commit associated with the release tag
    let release_commit = repo
        .get_tag_commit(&release_tag)
        .context("get release tag commit")?;

    // Checkout that commit in the worktree
    repo.checkout_commit(&release_commit)
        .context("checkout release commit for package")?;

    // Run cargo package so we have our finalized package
    run_cargo_package(&worktree).context("run cargo package")?;

    // Get the package metadata
    let single_package = get_cargo_package(&worktree, &package.name).with_context(|| {
        format!(
            "get cargo package {} from worktree at {:?}",
            package.name,
            worktree.path()
        )
    })?;

    let registry_package = RegistryPackage::new(single_package, Some(release_commit));
    Ok(Some((registry_package, worktree)))
}

/// Run cargo package within a worktree
fn run_cargo_package(worktree: &GitWorkTree) -> anyhow::Result<()> {
    let worktree_path = to_utf8_path(worktree.path())?;
    let output = run_cargo(worktree_path, &["package", "--allow-dirty"])
        .context("run cargo package in worktree")?;

    if !output.status.success() {
        anyhow::bail!("cargo package failed: {:?}", output.stderr);
    }

    Ok(())
}

fn get_cargo_package(worktree: &GitWorkTree, package_name: &str) -> anyhow::Result<Package> {
    let worktree_path = to_utf8_path(worktree.path())?;
    let manifest_path = worktree_path.join("Cargo.toml");

    // Use current_dir so that CARGO_TARGET_DIR resolves correctly relative to worktree
    let rust_package = MetadataCommand::new()
        .current_dir(worktree_path.as_std_path())
        .no_deps()
        .manifest_path(&manifest_path)
        .exec()
        .context("get cargo metadata for worktree")?;

    let package_details = rust_package
        .packages
        .iter()
        .find(|x| x.name == package_name)
        .with_context(|| format!("Failed to find package {package_name:?}"))?;

    let package_path = rust_package.target_directory.join(format!(
        "package/{}-{}",
        package_details.name, package_details.version
    ));
    debug!("package for {package_name} is at {package_path}");

    let single_package_manifest = package_path.join("Cargo.toml");
    let single_package_meta = get_manifest_metadata(&single_package_manifest)
        .context("get cargo metadata for package")?;

    let single_package = single_package_meta
        .workspace_packages()
        .into_iter()
        .find(|p| p.name == package_name)
        .context("Couldn't find the package")?
        .clone();

    Ok(single_package)
}

/// Determine next version of packages.
///
/// Returns:
/// - Any packages that need to be updated
/// - A temporary repository, i.e. an isolated copy of the repository used for git operations
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

    // Separate packages based on per-package git_only configuration
    let workspace_packages = input.cargo_metadata().workspace_packages();
    let (git_only_packages, registry_packages_list): (Vec<_>, Vec<_>) = workspace_packages
        .iter()
        .partition(|p| input.should_use_git_only(&p.name));

    let is_multi_package = local_project.publishable_packages().len() > 1;

    // Process git_only packages (version determined from git tags).
    // Worktrees must be kept alive until we're done with the packages.
    let (mut all_packages, _worktrees) =
        collect_git_only_packages(git_only_packages, input, is_multi_package)?;

    // Process registry packages (version determined from registry)
    let (registry_pkgs, registry_collection) = collect_registry_packages(
        registry_packages_list,
        &local_project.publishable_packages(),
        input,
    )?;
    all_packages.extend(registry_pkgs);

    // NOTE: We reuse registry_collection here instead of instantiating a new object
    // because otherwise the temp dir contained within it gets dropped and cleaned up.
    let release_packages = registry_collection.with_packages(all_packages);

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
        // This function runs inside a temporary repository, so this has no
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

/// Process all `git_only` packages and return their metadata.
///
/// Returns:
/// - A map of package name to `RegistryPackage`
/// - A list of worktrees that must be kept alive until we're done with the packages
fn collect_git_only_packages(
    git_only_packages: Vec<&Package>,
    input: &UpdateRequest,
    is_multi_package: bool,
) -> anyhow::Result<(BTreeMap<String, RegistryPackage>, Vec<GitWorkTree>)> {
    if git_only_packages.is_empty() {
        return Ok((BTreeMap::new(), Vec::new()));
    }

    debug!(
        "Processing {} packages in git_only mode",
        git_only_packages.len()
    );

    let mut all_packages = BTreeMap::new();
    // NOTE: We need to prevent the worktrees from being dropped because their Drop
    // implementation cleans up the worktrees.
    // See the note on the custom worktree Drop impl for more details.
    let mut worktrees = Vec::new();

    let mut unreleased_project_repo = GitRepo::open(
        input
            .local_manifest_dir()
            .context("get local manifest dir")?,
    )
    .context("create unreleased repo for spinning worktrees")?;

    for package in git_only_packages {
        if let Some((registry_package, worktree)) = process_git_only_package(
            package,
            &mut unreleased_project_repo,
            input,
            is_multi_package,
        )? {
            all_packages.insert(registry_package.package.name.to_string(), registry_package);
            worktrees.push(worktree);
        }
    }

    Ok((all_packages, worktrees))
}

/// Fetch packages from the registry and return their metadata.
///
/// Returns:
/// - A map of package name to `RegistryPackage`
/// - The `PackagesCollection` (must be kept alive because it owns the temp dir)
fn collect_registry_packages(
    registry_packages_list: Vec<&Package>,
    publishable_packages: &[&Package],
    input: &UpdateRequest,
) -> anyhow::Result<(BTreeMap<String, RegistryPackage>, PackagesCollection)> {
    if registry_packages_list.is_empty() {
        return Ok((BTreeMap::new(), PackagesCollection::default()));
    }

    debug!(
        "Processing {} packages from registry",
        registry_packages_list.len()
    );

    // Filter to only publishable packages
    let publishable_registry_packages: Vec<&Package> = registry_packages_list
        .into_iter()
        .filter(|p| {
            publishable_packages
                .iter()
                .any(|pub_pkg| pub_pkg.name == p.name)
        })
        .collect();

    if publishable_registry_packages.is_empty() {
        return Ok((BTreeMap::new(), PackagesCollection::default()));
    }

    // Retrieve the latest published version of the packages.
    // Release-plz will compare the registry packages with the local packages
    // to determine the new commits.
    let registry_packages = registry_packages::get_registry_packages(
        input.registry_manifest(),
        &publishable_registry_packages,
        input.registry(),
    )?;

    let mut all_packages = BTreeMap::new();
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

    Ok((all_packages, registry_packages))
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
    /// The last released/published version from the registry.
    /// This is set when the local version was already bumped (higher than registry version).
    /// Used to generate correct version transitions in PR body (e.g., "0.1.0 -> 0.2.0")
    /// instead of just showing "0.2.0" when `previous_version == next_version`.
    pub registry_version: Option<Version>,
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
