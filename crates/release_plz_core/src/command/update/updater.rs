use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, Once},
    thread,
};

use anyhow::Context as _;
use cargo::util::VersionExt as _;
use cargo_metadata::{
    Package, TargetKind,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_utils::LocalManifest;
use git_cliff_core::{
    config::{ChangelogConfig, Config},
    contributor::RemoteContributor,
};
use git_cmd::Repo;
use next_version::NextVersion as _;
use tracing::{debug, info, instrument, warn};

use crate::{
    ChangelogBuilder, ChangelogRequest, ForgeType, NO_COMMIT_ID, PackagePath as _, Project, Remote,
    RepoUrl, UpdateResult,
    changelog_filler::{fill_commit, get_required_info},
    changelog_parser,
    command::update::changelog_update::{OldChangelog, OldChangelogs},
    diff::{Commit, Diff},
    fs_utils, lock_compare,
    next_ver::takes_part_in_release,
    package_compare::{CARGO_TOML_ORIG, CARGO_VCS_INFO, PackageFiles},
    registry_packages::{PackagesCollection, RegistryPackage},
    semver_check::{self, SemverCheck},
    toml_compare,
    version::NextVersionFromDiff as _,
};

use super::{
    PackagesUpdate, package_dependencies::PackageDependencies as _, update_request::UpdateRequest,
};

mod history;

static SEMVER_CHECK_LOG_ONCE: Once = Once::new();

#[derive(Debug)]
pub struct Updater<'a> {
    pub project: &'a Project,
    pub req: &'a UpdateRequest,
}

/// Versions and release reasons are finalized before generating any changelogs.
struct PlannedUpdate<'a> {
    package: &'a Package,
    diff: Diff,
    version: Version,
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
        let version_groups = self.get_version_groups(&packages_diffs)?;
        debug!("version groups: {:?}", version_groups);
        let version_groups_with_release_commit =
            self.version_groups_with_release_commit(&packages_diffs);

        let mut packages_to_check_for_deps = Vec::new();
        let mut filtered_packages = HashSet::new();
        let mut planned_updates = Vec::new();

        let workspace_packages = crate::project::workspace_packages_at(
            self.req.cargo_metadata(),
            crate::manifest_dir(local_manifest_path)?,
        )?;
        let mut inheriting_packages = Vec::new();
        for package in &workspace_packages {
            if LocalManifest::try_new(&package.manifest_path)?.version_is_inherited() {
                inheriting_packages.push(package);
            }
        }
        let workspace_version_pkgs: HashSet<String> = inheriting_packages
            .iter()
            .map(|p| p.name.to_string())
            .collect();

        let new_workspace_version = self.new_workspace_version(
            local_manifest_path,
            &packages_diffs,
            &workspace_version_pkgs,
        )?;
        for (p, diff) in packages_diffs {
            let group_has_release_commit = || {
                self.req
                    .get_package_config(&p.name)
                    .version_group
                    .as_ref()
                    .is_some_and(|group| version_groups_with_release_commit.contains(group))
            };
            if let Some(release_commits_regex) = self.req.release_commits()
                && !diff.any_commit_matches(release_commits_regex)
                && !group_has_release_commit()
            {
                info!("{}: no commit matches the `release_commits` regex", p.name);
                // We need to update this package only if one of its dependencies has changed.
                filtered_packages.insert(p.name.as_str());
                packages_to_check_for_deps.push((p, diff));
                continue;
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
            // Process package if:
            // - Version changes.
            // - Package is new.
            // - Version was already bumped with pending unreleased commits so that we update the changelog.
            let version_already_bumped = !diff.is_version_published && !diff.commits.is_empty();
            if next_version != current_version
                || !diff.registry_package_exists
                || version_already_bumped
            {
                planned_updates.push(PlannedUpdate {
                    package: p,
                    diff,
                    version: next_version,
                });
            } else {
                // We need to update this package only if one of its dependencies or the
                // workspace version it inherits changes.
                // This includes already bumped (unpublished) versions without new commits:
                // a dependency change can still release them.
                packages_to_check_for_deps.push((p, diff));
            }
        }

        let workspace_version = self.dependent_packages_update(
            &packages_to_check_for_deps,
            &mut planned_updates,
            &inheriting_packages,
            &workspace_version_pkgs,
            &filtered_packages,
            new_workspace_version.as_ref(),
        )?;
        let mut old_changelogs =
            OldChangelogs::new(self.shared_changelog_paths(&workspace_packages));
        let updates = planned_updates
            .into_iter()
            .map(|update| self.calculate_update_result(update, &mut old_changelogs))
            .collect::<anyhow::Result<_>>()?;
        let mut packages_to_update = PackagesUpdate::new(updates);
        if let Some(version) = workspace_version {
            packages_to_update.with_workspace_version(version);
        }
        Ok(packages_to_update)
    }

    /// Changelogs that more than one of the `workspace_packages` writes to.
    fn shared_changelog_paths(&self, workspace_packages: &[Package]) -> HashSet<Utf8PathBuf> {
        let mut paths = HashSet::new();
        workspace_packages
            .iter()
            .map(|p| self.req.changelog_path(p))
            .filter(|path| !paths.insert(path.clone()))
            .collect()
    }

    /// Get the highest next version of all packages for each version group.
    fn get_version_groups(
        &self,
        packages_diffs: &[(&Package, Diff)],
    ) -> anyhow::Result<HashMap<String, Version>> {
        let mut version_groups: HashMap<String, Version> = HashMap::new();

        for (pkg, diff) in packages_diffs {
            let pkg_config = self.req.get_package_config(&pkg.name);
            let version_updater = pkg_config.generic.version_updater()?;
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

        Ok(version_groups)
    }

    fn version_groups_with_release_commit(
        &self,
        packages_diffs: &[(&Package, Diff)],
    ) -> HashSet<String> {
        let mut groups = HashSet::new();
        if let Some(release_commits_regex) = self.req.release_commits() {
            for (pkg, diff) in packages_diffs {
                let pkg_config = self.req.get_package_config(&pkg.name);
                if let Some(version_group) = pkg_config.version_group
                    && diff.any_commit_matches(release_commits_regex)
                {
                    groups.insert(version_group);
                }
            }
        }
        groups
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
        if workspace_version_pkgs.is_empty() {
            return Ok(None);
        }
        let mut new_versions = Vec::new();
        for (p, diff) in packages_diffs {
            if workspace_version_pkgs.contains(p.name.as_str()) {
                let pkg_config = self.req.get_package_config(&p.name);
                let version_updater = pkg_config.generic.version_updater()?;
                let next = p.version.next_from_diff(diff, version_updater);
                if let Some(workspace_version) = &workspace_version
                    && &next >= workspace_version
                {
                    new_versions.push(next);
                }
            }
        }
        Ok(new_versions.into_iter().max())
    }

    async fn get_packages_diffs(
        &self,
        registry_packages: &PackagesCollection,
        repository: &Repo,
    ) -> anyhow::Result<Vec<(&Package, Diff)>> {
        // Store diff for each package. This operation is not thread safe, so we do it in one
        // package at a time.

        let packages_diffs_res: anyhow::Result<Vec<(&Package, Diff)>> = self
            .packages_to_process()
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
            .map(|(p, d)| (p.name.to_string(), d.commits.clone()))
            .collect();

        let check_semver = |(p, diff): &mut (&Package, Diff)| -> anyhow::Result<()> {
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
                if should_check_semver(p, registry_package, package_config.semver_check())
                    && diff.should_update_version()
                {
                    let registry_package_path = registry_package
                        .package_path()
                        .context("can't retrieve registry package path")?;
                    // Log that we are checking semver only the first time.
                    SEMVER_CHECK_LOG_ONCE.call_once(|| {
                        tracing::info!("Checking API compatibility with cargo-semver-checks...");
                    });
                    let semver_check =
                        semver_check::run_semver_check(&package_path, registry_package_path)
                            .context("error while running cargo-semver-checks")?;
                    diff.set_semver_check(semver_check);
                }
            }
            Ok(())
        };

        // Worker count is capped at both the CPU count and the number of packages, so no idle threads are spawned.
        let parallelism = thread::available_parallelism()
            .map_or(1, usize::from)
            .min(packages_diffs.len());
        // Workers pull the next package from a shared queue, so a slow check
        // doesn't leave other workers idle.
        let queue = Mutex::new(packages_diffs.iter_mut());
        thread::scope(|scope| {
            let mut workers = Vec::with_capacity(parallelism);
            for _ in 0..parallelism {
                workers.push(scope.spawn(|| {
                    // Each worker repeatedly takes the next package from the shared queue until it’s empty.
                    loop {
                        // Release the queue lock before running the check.
                        let Some(package_diff) = queue.lock().unwrap().next() else {
                            break;
                        };
                        check_semver(package_diff)?;
                    }
                    anyhow::Ok(())
                }));
            }
            for worker in workers {
                worker.join().expect("semver check thread panicked")?;
            }
            anyhow::Ok(())
        })?;

        Ok(packages_diffs)
    }

    fn packages_to_process(&self) -> Vec<&Package> {
        self.project
            .workspace_packages()
            .into_iter()
            .filter(|p| takes_part_in_release(p, self.req.should_use_git_only(&p.name)))
            .collect()
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

    /// Propagate dependency and shared-version changes until no more packages need a release.
    /// Returns the new workspace version if a package inheriting it is released.
    fn dependent_packages_update<'a>(
        &self,
        packages_to_check_for_deps: &[(&'a Package, Diff)],
        planned_updates: &mut Vec<PlannedUpdate<'a>>,
        inheriting_packages: &[&Package],
        workspace_version_pkgs: &HashSet<String>,
        filtered_packages: &HashSet<&str>,
        initial_workspace_version: Option<&Version>,
    ) -> anyhow::Result<Option<Version>> {
        let workspace_manifest = LocalManifest::try_new(self.req.local_manifest())?;
        let workspace_dependencies = workspace_manifest.get_workspace_dependency_table();
        let workspace_dir = crate::manifest_dir(self.req.local_manifest())?;
        let mut processed: HashSet<&str> = planned_updates
            .iter()
            .map(|u| u.package.name.as_str())
            .collect();

        loop {
            // A dependency-only release can raise the shared version after the
            // initial commit-based calculation. Only activate it if an inheriting
            // package is actually being released, preserving release_commits filtering.
            let workspace_version = planned_updates
                .iter()
                .filter(|u| workspace_version_pkgs.contains(u.package.name.as_str()))
                .map(|u| &u.version)
                .max()
                .map(|version| version.max(initial_workspace_version.unwrap_or(version)))
                .cloned();
            if let Some(version) = &workspace_version {
                for update in planned_updates.iter_mut() {
                    if workspace_version_pkgs.contains(update.package.name.as_str()) {
                        update.version = version.clone();
                    }
                }
            }
            let mut changed_packages: Vec<(&Package, Version)> = planned_updates
                .iter()
                .map(|u| (u.package, u.version.clone()))
                .collect();
            if let Some(version) = &workspace_version {
                // Every inheriting member physically changes version, even if it isn't
                // released (e.g. filtered by release_commits or with `release = false`).
                // Its dependents must see the new version without releasing the member.
                // `update_manifests` updates their requirements with the same rule.
                for &p in inheriting_packages {
                    if !processed.contains(p.name.as_str()) && &p.version != version {
                        changed_packages.push((p, version.clone()));
                    }
                }
            }
            let mut any_package_updated = false;
            for (p, diff) in packages_to_check_for_deps {
                if processed.contains(p.name.as_str()) {
                    continue;
                }
                let inherited_version = workspace_version
                    .as_ref()
                    .filter(|_| workspace_version_pkgs.contains(p.name.as_str()));
                let Ok(deps) = p.dependencies_to_update(
                    &changed_packages,
                    workspace_dependencies,
                    workspace_dir,
                    self.req.should_use_git_only(&p.name),
                ) else {
                    continue;
                };
                let (change, version) = if !deps.is_empty() {
                    let deps: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
                    let next_version = if !diff.is_version_published {
                        p.version.clone()
                    } else if p.version.is_prerelease() {
                        p.version.increment_prerelease()
                    } else {
                        p.version.increment_patch()
                    };
                    (
                        format!(
                            "chore: updated the following local packages: {}",
                            deps.join(", ")
                        ),
                        inherited_version
                            .map_or(next_version.clone(), |v| next_version.max(v.clone())),
                    )
                } else if let Some(version) = inherited_version
                    && version != &p.version
                    && !filtered_packages.contains(p.name.as_str())
                {
                    // Unfiltered packages with commits are always planned already,
                    // so this package has no commits to keep.
                    (
                        "chore: updated workspace version".to_string(),
                        version.clone(),
                    )
                } else {
                    continue;
                };
                let update = PlannedUpdate {
                    package: p,
                    diff: Diff {
                        commits: vec![Commit::new(NO_COMMIT_ID.to_string(), change)],
                        semver_check: SemverCheck::Skipped,
                        ..diff.clone()
                    },
                    version,
                };
                if let Some((_, version)) = changed_packages
                    .iter_mut()
                    .find(|(changed, _)| changed.id == p.id)
                {
                    *version = update.version.clone();
                } else {
                    changed_packages.push((p, update.version.clone()));
                }
                planned_updates.push(update);
                processed.insert(p.name.as_str());
                any_package_updated = true;
            }
            if !any_package_updated {
                return Ok(workspace_version);
            }
        }
    }

    fn calculate_update_result(
        &self,
        update: PlannedUpdate<'_>,
        old_changelogs: &mut OldChangelogs,
    ) -> anyhow::Result<(Package, UpdateResult)> {
        let PlannedUpdate {
            package: p,
            diff,
            version,
        } = update;
        let action = if version == p.version && !diff.is_version_published {
            "updating changelog for version"
        } else {
            "next version is"
        };
        info!(
            "{}: {action} {version}{}",
            p.name,
            diff.semver_check.outcome_str()
        );
        let changelog_path = self.req.changelog_path(p);
        let old_changelog = old_changelogs.get_or_read(&changelog_path);
        let update_result = self.update_result(
            diff.commits,
            version,
            p,
            diff.semver_check,
            diff.registry_version,
            old_changelog,
        )?;
        if let Some(changelog) = &update_result.changelog {
            old_changelog.update(changelog.clone());
        }
        Ok((p.clone(), update_result))
    }

    /// This function needs `old_changelog` so that you can have changes of different
    /// packages in the same changelog.
    fn update_result(
        &self,
        commits: Vec<Commit>,
        version: Version,
        package: &Package,
        semver_check: SemverCheck,
        registry_version: Option<Version>,
        old_changelog: &OldChangelog,
    ) -> anyhow::Result<UpdateResult> {
        let repo_url = self.req.repo_url();
        // Use registry_version as the previous version when available (version already
        // bumped case), otherwise use package.version (normal case)
        let previous_version = registry_version.as_ref().unwrap_or(&package.version);
        let changelog_repo = {
            let prev_tag = self
                .project
                .git_tag(&package.name, &previous_version.to_string())?;
            let next_tag = self.project.git_tag(&package.name, &version.to_string())?;
            repo_url.map(|url| ChangelogRepo {
                url,
                forge: self.req.forge_type(),
                release_link: url.git_release_link(&prev_tag, &next_tag),
            })
        };

        let changelog_outcome = {
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
                        changelog_repo,
                        package,
                        previous_version,
                    )
                })
                .transpose()
        }?;

        let (changelog, new_changelog_entry) = match changelog_outcome {
            Some((changelog, new_changelog_entry)) => (Some(changelog), Some(new_changelog_entry)),
            None => (None, None),
        };

        Ok(UpdateResult {
            version,
            changelog,
            semver_check,
            new_changelog_entry,
            registry_version,
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
        info!(
            "determining next version for {} {}",
            package.name, package.version
        );
        let package_path = get_package_path(package, repository, self.project.root())
            .context("failed to determine package path")?;

        repository
            .checkout_head()
            .context("can't checkout head to calculate diff")?;
        let registry_package = registry_packages.get_registry_package(&package.name);
        let mut diff = Diff::new(registry_package.is_some());
        let git_tag = self
            .project
            .git_tag(&package.name, &package.version.to_string())?;
        let tag_commit = repository.get_tag_commit(&git_tag);

        // Check if git_only is enabled for this package
        let using_git_only = || self.req.should_use_git_only(&package.name);

        if tag_commit.is_some() && !using_git_only() {
            // Only check registry for packages that should be published
            // Skip this check if git_only is enabled (we don't use registry in that mode)
            let config = self.req.get_package_config(&package.name);
            if config.should_publish() {
                let registry_package = registry_package.with_context(|| format!("package `{}` not found in the registry, but the git tag {git_tag} exists. Consider running `cargo publish` manually to publish this package.", package.name))?;
                anyhow::ensure!(
                    package.version <= registry_package.package.version,
                    "local package `{}` has a greater version ({}) with respect to the registry package ({}), but the git tag {git_tag} exists. Consider running `cargo publish` manually to publish the new version of this package.",
                    package.name,
                    package.version,
                    registry_package.package.version
                );
            }
        }
        self.get_package_diff(
            &package_path,
            package,
            registry_package,
            repository,
            tag_commit.as_deref(),
            &mut diff,
        )?;

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
        let released_package_files = PackageFiles::default();
        // The released package, paired with the path of its extracted sources.
        let released = registry_package
            .map(|p| p.package.package_path().map(|path| (p, path)))
            .transpose()?;
        // A workspace-only version bump may have no commits in this package's
        // paths. Recognize it before the walk so dependency updates preserve it.
        if let Some((released_package, _)) = released
            && package.version > released_package.package.version
        {
            info!(
                "{}: local version ({}) > registry version ({}). Only changelog will be updated.",
                package.name, package.version, released_package.package.version
            );
            diff.set_version_unpublished(released_package.package.version.clone());
        }
        let paths = PackagePaths::new(package_path, package)?;
        let max_analyze_commits = released
            .is_none()
            .then(|| self.req.max_analyze_commits())
            // 0 means "no limit"
            .filter(|&n| n != 0);
        // Exclude already released history using both the release tag and the
        // registry's published commit, when available. The walk skips these
        // commits and their ancestors.
        let release_boundaries: Vec<&str> = tag_commit
            .into_iter()
            .chain(released.and_then(|(p, _)| p.published_at_sha1()))
            .collect();
        let head = repository.current_commit_hash()?;
        // Enumerate from the branch tip before checking out any historical snapshot.
        // The parents let RetainedChanges follow the lineages of this same walk.
        let graph = repository.parents_at_paths(
            &head,
            &release_boundaries,
            &paths.all(),
            max_analyze_commits,
        )?;
        let mut retained_changes =
            history::RetainedChanges::new(repository, &head, &graph, &paths)?;
        for (current_commit_hash, _) in graph {
            // Stop lineages that have reached an equal snapshot. Still inspect
            // ancestors reachable through another lineage: they can contain
            // surviving changes or another equal snapshot that bounds that lineage.
            // Without a tag or a published commit bounding the walk, a branch that
            // changes the package, forked before the release and merged after it,
            // keeps its fork point and every ancestor of it reachable, so all of
            // those are inspected.
            if retained_changes.skips(&current_commit_hash) {
                continue;
            }
            checkout_commit(repository, &current_commit_hash)?;
            // Equality and changed-file checks inspect the same snapshot.
            let local_package_files = PackageFiles::default();
            if let Some((released_package, released_path)) = released {
                let are_packages_equal = self.check_package_equality(
                    repository,
                    package,
                    package_path,
                    released_package,
                    released_path,
                    (&local_package_files, &released_package_files),
                ).with_context(|| format!("failed to check package equality for `{}` at commit {current_commit_hash}", package.name))?;
                if are_packages_equal {
                    // Collect pruning candidates with full history: a "keep mine"
                    // merge can hide real ancestors from a simplified walk. Reuse
                    // the outer walk's paths and release boundaries to avoid
                    // collecting history already excluded from consideration.
                    // RetainedChanges preserves candidates whose changes survive
                    // through another lineage.
                    let ancestors = repository.ancestors_at_paths(
                        &current_commit_hash,
                        &release_boundaries,
                        &paths.all(),
                    )?;
                    retained_changes.add_boundary(
                        &current_commit_hash,
                        ancestors,
                        released_package_files.get(released_path)?,
                    );
                    continue;
                }
            }
            // A package can contain another package in a subdirectory, so only count
            // commits that touch files Cargo would package for this package.
            if self.are_changed_files_in_package(
                package_path,
                repository,
                &current_commit_hash,
                &local_package_files,
            )? {
                diff.commits.push(Commit::new(
                    current_commit_hash,
                    repository.current_commit_message()?,
                ));
            }
        }
        repository
            .checkout_head()
            .context("can't checkout head to compare dependencies")?;
        // A simplified walk can visit an ancestor before the equal snapshot that
        // prunes it. Make the final decision with every discovered boundary,
        // keeping only ancestors whose changes survive through another lineage.
        retained_changes.retain_surviving(&mut diff.commits, || {
            self.history_package_files(package_path, repository)
        })?;
        // The range can be empty when only workspace Cargo.toml or Cargo.lock
        // changed. Dependency updates must not depend on visiting a package commit.
        if diff.commits.is_empty()
            && let Some((released_package, released_path)) = released
        {
            self.add_dependencies_update_if_any(diff, released_package, package, released_path)?;
        }
        Ok(())
    }

    /// Whether the current checkout equals the released package, README included.
    fn check_package_equality(
        &self,
        repository: &Repo,
        package: &Package,
        package_path: &Utf8Path,
        registry_package: &RegistryPackage,
        registry_package_path: &Utf8Path,
        (local_package_files, released_package_files): (&PackageFiles, &PackageFiles),
    ) -> anyhow::Result<bool> {
        let packages_equal = self
            .with_cargo_lock_restored(repository, || {
                crate::package_compare::are_packages_equal_cached(
                    package_path,
                    registry_package_path,
                    local_package_files,
                    released_package_files,
                )
            })?
            .context("cannot compare packages");
        // Most historical snapshots already differ in their packaged files.
        // Read README metadata only when that comparison cannot decide equality
        // (because `cargo metadata` is slower).
        if matches!(packages_equal, Ok(false)) {
            return Ok(false);
        }
        if crate::package_compare::is_readme_updated_with_released_package(
            &package.name,
            package_path,
            &registry_package.package,
        )? {
            debug!("{}: README updated", package.name);
            return Ok(false);
        }
        // A README change establishes inequality even if package listing failed.
        packages_equal
    }

    /// If the dependencies changed, add a commit to the diff.
    fn add_dependencies_update_if_any(
        &self,
        diff: &mut Diff,
        registry_package: &RegistryPackage,
        package: &Package,
        registry_package_path: &Utf8Path,
    ) -> anyhow::Result<()> {
        let are_toml_dependencies_updated = || {
            toml_compare::are_toml_dependencies_updated(
                &registry_package.package.dependencies,
                &package.dependencies,
            )
        };
        let are_lock_dependencies_updated = || {
            if let Some(released_workspace) = registry_package.released_workspace() {
                lock_compare::are_workspace_lock_dependencies_updated(
                    self.req.cargo_metadata(),
                    released_workspace,
                    &package.name,
                )
            } else {
                lock_compare::are_lock_dependencies_updated(
                    self.req.cargo_metadata(),
                    registry_package_path,
                    &package.name,
                )
            }
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

    /// Run `f`, which inspects the package with `cargo package`, then revert the
    /// edits `cargo package` can make to files such as `Cargo.lock`.
    fn with_cargo_lock_restored<T>(
        &self,
        repository: &Repo,
        f: impl FnOnce() -> T,
    ) -> anyhow::Result<T> {
        // Store the path before `f` runs so it can be reverted afterwards.
        let cargo_lock_path = self
            .get_cargo_lock_path(repository)
            .context("failed to determine Cargo.lock path")?;
        let result = f();
        if let Some(cargo_lock_path) = cargo_lock_path.as_deref() {
            repository
                .checkout(cargo_lock_path)
                .context("cannot revert changes introduced when comparing packages")?;
        }
        Ok(result)
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
                    let version_updater = pkg_config.generic.version_updater()?;
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
        package_files: &PackageFiles,
    ) -> anyhow::Result<bool> {
        let get_files = || get_package_files(package_path, repository, package_files);
        let package_files_res = if package_files.is_cached() {
            // Equality already listed this snapshot's files and restored the lockfile.
            // Reading the cached list cannot change Cargo.lock.
            get_files()
        } else {
            self.with_cargo_lock_restored(repository, get_files)?
        };
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

    /// List the files Cargo packages in the current checkout, relative to the
    /// package directory, restoring any existing Cargo.lock after listing.
    ///
    /// Return `None` if listing fails, so history comparisons fall back to every
    /// file under the package directory.
    fn history_package_files(
        &self,
        package_path: &Utf8Path,
        repository: &Repo,
    ) -> anyhow::Result<Option<Vec<Utf8PathBuf>>> {
        let package_files = self.with_cargo_lock_restored(repository, || {
            crate::get_cargo_package_files(package_path)
        })?;
        // Cargo also lists generated files that do not exist in the checkout.
        // Tree comparisons only need their names, not canonicalized files.
        Ok(package_files
            .inspect_err(|error| debug!("cannot list files for history comparison: {error:#}"))
            .ok())
    }
}

/// Checkout a commit of the history we are walking, hinting at `--allow-dirty` when
/// uncommitted changes are what stopped the checkout.
fn checkout_commit(repository: &Repo, commit: &str) -> anyhow::Result<()> {
    repository.checkout(commit).map_err(|err| {
        // git reports this in the stderr of the innermost error, so look at the
        // whole chain rather than at the outermost context.
        if format!("{err:#}")
            .contains("Your local changes to the following files would be overwritten")
        {
            err.context("The allow-dirty option can't be used in this case")
        } else {
            err.context(format!("failed to checkout commit {commit}"))
        }
    })
}

/// Check if release-plz should check the semver compatibility of the package.
/// - `run_semver_check` is true if the user wants to run the semver check.
fn should_check_semver(
    package: &Package,
    registry_package: &Package,
    run_semver_check: bool,
) -> bool {
    // Adding a library to a binary-only package has no previous library API to compare.
    if run_semver_check && contains_library(package) && contains_library(registry_package) {
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
    target_kinds(package).any(|kind| *kind == TargetKind::Bin)
}

fn contains_library(package: &Package) -> bool {
    // `rlib` and `dylib` are Rust libraries like `lib`: downstream Rust crates can depend on
    // them, so their API is subject to semver. `cdylib` and `staticlib` only expose a C ABI.
    target_kinds(package)
        .any(|kind| matches!(kind, TargetKind::Lib | TargetKind::RLib | TargetKind::DyLib))
}

/// Kinds of all the targets of the package.
/// We use target `kind` because target `crate_types` contains "Bin" if the kind is "Test".
fn target_kinds(package: &Package) -> impl Iterator<Item = &TargetKind> {
    package.targets.iter().flat_map(|t| t.kind.iter())
}

/// Get files that belong to the package.
/// The paths are relative to the git repo root.
fn get_package_files(
    package_path: &Utf8Path,
    repository: &Repo,
    package_files: &PackageFiles,
) -> anyhow::Result<HashSet<Utf8PathBuf>> {
    // Get relative path of the crate with respect to the repository because we need to compare
    // files with the git output.
    let repository_dir = repository.directory();

    package_files
        .get(package_path)?
        .iter()
        // filter file generated by `cargo package` that isn't in git.
        .filter(|file| file.as_str() != CARGO_TOML_ORIG && file.as_str() != CARGO_VCS_INFO)
        .map(|file| {
            // Normalize path to handle symbolic links correctly.
            let file_path = package_path.join(file);
            let normalized = fs_utils::canonicalize_utf8(&file_path)?;
            let relative_path = normalized
                .strip_prefix(repository_dir)
                .with_context(|| format!("failed to strip {repository_dir} from {normalized}"))?;
            Ok(relative_path.to_path_buf())
        })
        .collect()
}

/// The paths whose history holds a package's changes.
#[derive(Debug)]
struct PackagePaths {
    /// The package directory.
    package: Utf8PathBuf,
    /// The canonical target of the README configured in `Cargo.toml`, when it
    /// exists: it can live outside the package directory.
    readme: Option<Utf8PathBuf>,
}

impl PackagePaths {
    fn new(package_path: &Utf8Path, package: &Package) -> anyhow::Result<Self> {
        Ok(Self {
            package: package_path.to_path_buf(),
            readme: crate::local_readme_override(package, package_path)?,
        })
    }

    /// Every path, for path-limited Git commands.
    fn all(&self) -> Vec<&Utf8Path> {
        std::iter::once(self.package.as_path())
            .chain(self.readme.as_deref())
            .collect()
    }
}

struct ChangelogRepo<'a> {
    url: &'a RepoUrl,
    forge: ForgeType,
    release_link: String,
}

/// Return the following tuple:
/// - the entire changelog (with the new entries);
/// - the new changelog entry alone
///   (i.e. changelog body update without header and footer).
///
/// `previous_version` is the version `package` is released from, used when the
/// old changelog can't tell it.
fn get_changelog(
    commits: &[Commit],
    next_version: &Version,
    changelog_req: Option<ChangelogRequest>,
    old_changelog: &OldChangelog,
    repo: Option<ChangelogRepo<'_>>,
    package: &Package,
    previous_version: &Version,
) -> anyhow::Result<(String, String)> {
    let commits: Vec<git_cliff_core::commit::Commit> =
        commits.iter().map(|c| c.to_cliff_commit()).collect();
    let mut changelog_builder = ChangelogBuilder::new(
        commits.clone(),
        next_version.to_string(),
        package.name.to_string(),
    );
    if let Some(changelog_req) = changelog_req {
        if let Some(release_date) = changelog_req.release_date {
            changelog_builder = changelog_builder.with_release_date(release_date);
        }
        if let Some(config) = changelog_req.changelog_config {
            changelog_builder = changelog_builder.with_config(config);
        }
        if let Some(repo) = repo {
            changelog_builder = changelog_builder.with_release_link(repo.release_link);
            let repo_url = repo.url;
            let remote = Remote {
                owner: repo_url.owner.clone(),
                repo: repo_url.name.clone(),
                link: repo_url.full_host(),
                contributors: get_contributors(&commits),
            };
            changelog_builder = changelog_builder.with_remote(remote);

            let pr_link = repo_url.git_pr_link_for(repo.forge);
            changelog_builder = changelog_builder.with_pr_link(pr_link);
        }
        let is_package_published = next_version != &package.version;

        let last_version = old_changelog.original().and_then(|old_changelog| {
            changelog_parser::last_version_from_str(old_changelog)
                .ok()
                .flatten()
        });
        if is_package_published {
            // The latest release of a shared changelog can belong to another package.
            let last_version = last_version
                .filter(|_| !old_changelog.is_shared())
                .unwrap_or_else(|| previous_version.to_string());
            changelog_builder = changelog_builder.with_previous_version(last_version);
        } else if let Some(last_version) = last_version
            && let Some(old_changelog) = old_changelog.current()
            && last_version == next_version.to_string()
        {
            // If the next version is the same as the last version of the changelog,
            // don't update the changelog (returning the old one).
            // This can happen when no version of the package was published,
            // but the changelog already contains the changes of the initial version
            // of the package (e.g. because a release PR was merged).
            return Ok((old_changelog.to_string(), String::new()));
        }
    }
    let new_changelog = changelog_builder.build();
    let changelog = match old_changelog.current() {
        Some(old_changelog) => new_changelog.prepend(old_changelog)?,
        None => new_changelog.generate()?, // Old changelog doesn't exist.
    };
    let body_only =
        new_changelog_entry(changelog_builder).context("can't determine changelog body")?;
    Ok((changelog, body_only.unwrap_or_default()))
}

fn new_changelog_entry(changelog_builder: ChangelogBuilder) -> anyhow::Result<Option<String>> {
    changelog_builder
        .config()
        .cloned()
        .map(|c| {
            let new_config = Config {
                changelog: ChangelogConfig {
                    // If we set None, later this will be overriden with the defaults.
                    // Instead we just want the body.
                    header: Some(String::new()),
                    footer: Some(String::new()),
                    ..c.changelog
                },
                ..c
            };
            let changelog = changelog_builder.with_config(new_config).build();
            changelog.generate().map(|entry| entry.trim().to_string())
        })
        .transpose()
}

fn get_contributors(commits: &[git_cliff_core::commit::Commit]) -> Vec<RemoteContributor> {
    let mut unique_contributors = HashSet::new();
    commits
        .iter()
        .filter_map(|c| c.remote.clone())
        .filter(|remote| remote.username.is_some())
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
    fn contributors_are_unique_and_have_usernames() {
        let commits = [
            (None, None),
            (None, Some(42)),
            (None, Some(43)),
            (Some("alice"), Some(44)),
            (Some("alice"), Some(45)),
            (Some("bob"), Some(46)),
        ]
        .map(|(username, pr_number)| Commit {
            remote: RemoteContributor {
                username: username.map(str::to_string),
                pr_number,
                ..RemoteContributor::default()
            },
            ..Commit::default()
        });
        let commits: Vec<_> = commits.iter().map(Commit::to_cliff_commit).collect();

        assert_eq!(commits[1].remote.as_ref().unwrap().pr_number, Some(42));
        assert!(get_contributors(&commits[..3]).is_empty());

        let contributors = get_contributors(&commits);
        let contributors: Vec<_> = contributors
            .iter()
            .map(|remote| (remote.username.as_deref(), remote.pr_number))
            .collect();
        assert_eq!(
            contributors,
            vec![(Some("alice"), Some(44)), (Some("bob"), Some(46))]
        );
    }

    #[test]
    fn only_rust_library_targets_are_libraries() {
        // (target kind, is semver-checked as a Rust library)
        for (kind, is_library) in [
            ("lib", true),
            ("rlib", true),
            ("dylib", true),
            ("bin", false),
            ("cdylib", false),
            ("staticlib", false),
            ("proc-macro", false),
            ("example", false),
            ("test", false),
            ("bench", false),
            ("custom-build", false),
        ] {
            let package: Package = fake_package::FakePackage::new("my_package")
                .with_targets(&[kind])
                .into();
            assert_eq!(contains_library(&package), is_library, "kind: {kind}");
        }
    }

    #[test]
    fn target_with_cdylib_and_rlib_kinds_is_a_library() {
        // `crate-type = ["cdylib", "rlib"]` is reported by cargo as one target with two kinds.
        let mut package: Package = fake_package::FakePackage::new("my_package")
            .with_targets(&["cdylib"])
            .into();
        package.targets[0].kind.push(TargetKind::RLib);
        assert!(contains_library(&package));
    }

    #[test]
    fn same_version_is_not_added_to_changelog() {
        let commits = vec![
            Commit::new(crate::NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
            Commit::new(crate::NO_COMMIT_ID.to_string(), "simple update".to_string()),
        ];

        let next_version = Version::new(1, 1, 0);
        let changelog_req = ChangelogRequest::default();

        let old = r"## [1.1.0] - 1970-01-01

### fix bugs
- my awesomefix

### other
- complex update
";
        let mut package: Package = fake_package::FakePackage::new("my_package").into();
        // Check both normal and manually bumped versions. For the latter, also check
        // when another package has already added its own entry to the shared changelog.
        for (package_version, updated) in [
            (package.version.clone(), None),
            (next_version.clone(), None),
            (
                next_version.clone(),
                Some(format!("## [2.0.0]\n\n- another package\n\n{old}")),
            ),
        ] {
            package.version = package_version;
            let mut old_changelog = OldChangelog::new(Some(old.to_string()), updated.is_some());
            if let Some(updated) = updated {
                old_changelog.update(updated);
            }
            let new = get_changelog(
                &commits,
                &next_version,
                Some(changelog_req.clone()),
                &old_changelog,
                None,
                &package,
                &package.version,
            )
            .unwrap();
            assert_eq!(old_changelog.current().unwrap(), new.0);
        }
    }
}
