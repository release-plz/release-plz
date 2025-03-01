use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::Context as _;
use cargo::util::VersionExt as _;
use cargo_metadata::{
    Package, TargetKind,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_utils::{CARGO_TOML, LocalManifest};
use git_cliff_core::contributor::RemoteContributor;
use git_cmd::Repo;
use next_version::NextVersion as _;
use rayon::iter::{IntoParallelRefMutIterator as _, ParallelIterator as _};
use tracing::{debug, info, instrument, warn};

use crate::{
    ChangelogBuilder, ChangelogRequest, NO_COMMIT_ID, PackagePath as _, Project, Remote, RepoUrl,
    UpdateResult,
    changelog_filler::{fill_commit, get_required_info},
    changelog_parser,
    command::update::changelog_update::OldChangelogs,
    diff::{Commit, Diff},
    fs_utils, lock_compare,
    registry_packages::{PackagesCollection, RegistryPackage},
    semver_check::{self, SemverCheck},
    toml_compare,
    version::NextVersionFromDiff as _,
};

use super::{
    PackagesToUpdate, PackagesUpdate, package_dependencies::PackageDependencies as _,
    update_request::UpdateRequest,
};

pub struct Updater<'a> {
    pub project: &'a Project,
    pub req: &'a UpdateRequest,
}

impl Updater<'_> {
    #[instrument(skip_all)]
    pub async fn packages_to_update(
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
            if let Some(release_commits_regex) = self.req.release_commits() {
                if !diff.any_commit_matches(release_commits_regex) {
                    info!("{}: no commit matches the `release_commits` regex", p.name);
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
                    .with_context(|| {
                        format!("failed to retrieve difference of package {}", p.name)
                    })?;
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
        let changelog_request: &ChangelogRequest = self.req.changelog_req();
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
                    .await
                    .context(
                        "Failed to fetch the commit information required by the changelog template",
                    )?;
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
        let workspace_dir = crate::manifest_dir(self.req.local_manifest())?;
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
        let next_version = if p.version.is_prerelease() {
            p.version.increment_prerelease()
        } else {
            p.version.increment_patch()
        };
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
        let repo_url = self.req.repo_url();
        let release_link = {
            let prev_tag = self
                .project
                .git_tag(&package.name, &package.version.to_string());
            let next_tag = self.project.git_tag(&package.name, &version.to_string());
            repo_url.map(|r| r.git_release_link(&prev_tag, &next_tag))
        };

        let changelog = {
            let cfg = self.req.get_package_config(package.name.as_str());
            let changelog_req = cfg
                .should_update_changelog()
                .then_some(self.req.changelog_req().clone());
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
        let registry_package = registry_packages.get_registry_package(&package.name);
        let mut diff = Diff::new(registry_package.is_some());
        let pathbufs_to_check = pathbufs_to_check(&package_path, package);
        let paths_to_check: Vec<&Path> = pathbufs_to_check.iter().map(|p| p.as_ref()).collect();
        repository
            .checkout_last_commit_at_paths(&paths_to_check)
            .map_err(|err| {
                if err
                    .to_string()
                    .contains("Your local changes to the following files would be overwritten")
                {
                    err.context("The allow-dirty option can't be used in this case")
                } else {
                    err.context("Failed to retrieve the last commit of local repository.")
                }
            })?;

        let git_tag = self
            .project
            .git_tag(&package.name, &package.version.to_string());
        let tag_commit = repository.get_tag_commit(&git_tag);
        if tag_commit.is_some() {
            let registry_package = registry_package.with_context(|| format!("package `{}` not found in the registry, but the git tag {git_tag} exists. Consider running `cargo publish` manually to publish this package.", package.name))?;
            anyhow::ensure!(
                registry_package.package.version == package.version,
                "package `{}` has a different version ({}) with respect to the registry package ({}), but the git tag {git_tag} exists. Consider running `cargo publish` manually to publish the new version of this package.",
                package.name,
                package.version,
                registry_package.package.version
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
        registry_package: Option<&RegistryPackage>,
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
                    debug!(
                        "next version calculated starting from commits after `{current_commit_hash}`"
                    );
                    if diff.commits.is_empty() {
                        // Even if the packages are equal, the Cargo.lock or Cargo.toml of the
                        // workspace might have changed.
                        // If the dependencies changed, we add a commit to the diff.
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
                    // We can process the next package.
                    break;
                } else if registry_package.package.version != package.version {
                    info!(
                        "{}: the local package has already a different version with respect to the registry package, so release-plz will not update it",
                        package.name
                    );
                    diff.set_version_unpublished();
                    break;
                } else if are_changed_files_in_pkg()? {
                    debug!("packages contain different files");
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
        if crate::is_readme_updated(&package.name, package_path, registry_package_path)? {
            debug!("{}: README updated", package.name);
            return Ok(false);
        }
        // We run `cargo package` when comparing packages, which can edit files, such as `Cargo.lock`.
        // Store its path so it can be reverted after comparison.
        let cargo_lock_path = self
            .get_cargo_lock_path(repository)
            .context("failed to determine Cargo.lock path")?;
        let are_packages_equal = crate::are_packages_equal(package_path, registry_package_path)
            .context("cannot compare packages")?;
        if let Some(cargo_lock_path) = cargo_lock_path.as_deref() {
            // Revert any changes to `Cargo.lock`
            repository
                .checkout(cargo_lock_path)
                .context("cannot revert changes introduced when comparing packages")?;
        }
        Ok(are_packages_equal)
    }

    /// If the dependencies changed, add a commit to the diff.
    fn add_dependencies_update_if_any(
        &self,
        diff: &mut Diff,
        registry_package: &Package,
        package: &Package,
        registry_package_path: &Utf8Path,
    ) -> anyhow::Result<()> {
        let are_toml_dependencies_updated = || {
            toml_compare::are_toml_dependencies_updated(
                &registry_package.dependencies,
                &package.dependencies,
            )
        };
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
        } else if contains_executable(package) && are_lock_dependencies_updated()? {
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
        let relative_lock_path = fs_utils::strip_prefix(&project_cargo_lock, self.project.root())?;
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

/// Check if release-plz should check the semver compatibility of the package.
/// - `run_semver_check` is true if the user wants to run the semver check.
fn should_check_semver(package: &Package, run_semver_check: bool) -> bool {
    if run_semver_check && contains_library(package) {
        let is_cargo_semver_checks_installed = semver_check::is_cargo_semver_checks_installed();
        if !is_cargo_semver_checks_installed {
            warn!(
                "cargo-semver-checks not installed, skipping semver check. For more information, see https://release-plz.dev/docs/semver-check"
            );
        }
        return is_cargo_semver_checks_installed;
    }
    false
}

fn contains_executable(package: &Package) -> bool {
    contains_target_kind(package, &TargetKind::Bin)
}

fn contains_library(package: &Package) -> bool {
    contains_target_kind(package, &TargetKind::Lib)
}

fn contains_target_kind(package: &Package, target_kind: &TargetKind) -> bool {
    // We use target `kind` because target `crate_types` contains "Bin" if the kind is "Test".
    package.targets.iter().any(|t| t.kind.contains(target_kind))
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
    let sources = crate::get_cargo_package_files(package_path)?
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
            debug!(
                "stopping looking at git history because the current commit ({}) is an ancestor of the commit ({}) tagged with the previous version.",
                current_commit_hash, tag_commit
            );
            return true;
        }
    }

    if let Some(published_commit) = published_at_commit.as_ref() {
        if repository.is_ancestor(current_commit_hash, published_commit) {
            debug!(
                "stopping looking at git history because the current commit ({}) is an ancestor of the commit ({}) where the previous version was published.",
                current_commit_hash, published_commit
            );
            return true;
        }
    }
    false
}

fn pathbufs_to_check(package_path: &Utf8Path, package: &Package) -> Vec<Utf8PathBuf> {
    let mut paths = vec![package_path.to_path_buf()];
    if let Some(readme_path) = crate::local_readme_override(package, package_path) {
        paths.push(readme_path);
    }
    paths
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

            let pr_link = repo_url.git_pr_link();
            changelog_builder = changelog_builder.with_pr_link(pr_link);
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
    let relative_path = fs_utils::strip_prefix(old_path, project_root)
        .context("error while retrieving package_path")?;
    let result_path = repository.directory().join(relative_path);

    Ok(result_path)
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
