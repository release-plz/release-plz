use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::Context as _;
use cargo_metadata::{
    Package, TargetKind,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use next_version::VersionUpdater;
use cargo_utils::LocalManifest;
use git_cliff_core::{
    config::{ChangelogConfig, Config},
    contributor::RemoteContributor,
};
use git_cmd::Repo;
use rayon::iter::{IntoParallelRefMutIterator as _, ParallelIterator as _};
use std::sync::Once;
use tracing::{debug, info, instrument, warn};

use crate::{
    ChangelogBuilder, ChangelogRequest, PackagePath as _, Project, Remote, RepoUrl,
    UpdateResult,
    changelog_filler::{fill_commit, get_required_info},
    changelog_parser,
    diff::{Commit, Diff},
    fs_utils,
    semver_check,
};

use super::{
    PackagesUpdate,
    update_request::UpdateRequest,
};

static SEMVER_CHECK_LOG_ONCE: Once = Once::new();

#[derive(Debug)]
pub struct Updater<'a> {
    pub project: &'a Project,
    pub req: &'a UpdateRequest,
}

impl Updater<'_> {
    #[instrument(skip_all)]
    pub async fn packages_to_update(
        &self,
        repository: &Repo,
        local_manifest_path: &Utf8Path,
    ) -> anyhow::Result<PackagesUpdate> {
        debug!("calculating unified workspace version");

        let packages_diffs = self
            .get_packages_diffs(repository)
            .await?;

        // For unified workspace versioning: collect ALL commits from ALL packages
        let mut all_commits: Vec<Commit> = packages_diffs
            .iter()
            .flat_map(|(_, diff)| diff.commits.clone())
            .collect();

        // Filter commits based on release_commits regex if configured
        if let Some(release_commits_regex) = self.req.release_commits() {
            let original_count = all_commits.len();
            all_commits.retain(|commit| release_commits_regex.is_match(&commit.message));
            debug!(
                "filtered commits from {} to {} based on release_commits regex",
                original_count,
                all_commits.len()
            );
        }

        // Check if workspace tag exists (same for all packages in unified versioning)
        let workspace_tag_exists = packages_diffs
            .first()
            .map(|(_, diff)| diff.tag_exists)
            .unwrap_or(false);

        debug!(
            "collected {} commits from {} packages, tag_exists: {}",
            all_commits.len(),
            packages_diffs.len(),
            workspace_tag_exists
        );

        let mut packages_to_update = PackagesUpdate::default();

        // If there are commits or no tag exists, calculate new workspace version
        // Note: When release_commits filter is configured and filters out all commits,
        // we should NOT create a release even if there's no tag (respects user's intent)
        let should_update = if self.req.release_commits().is_some() {
            // When release_commits is configured, only update if there are matching commits
            !all_commits.is_empty()
        } else {
            // Normal behavior: update if there are commits OR it's first release
            !all_commits.is_empty() || !workspace_tag_exists
        };

        if should_update {
            // Calculate ONE workspace version based on ALL commits
            let workspace_version = self.calculate_unified_workspace_version(
                local_manifest_path,
                &all_commits,
            )?;

            info!("unified workspace version: {workspace_version}");
            packages_to_update.with_workspace_version(workspace_version.clone());

            // Generate ONE workspace changelog for ALL packages
            let workspace_changelog = self.generate_workspace_changelog(
                &all_commits,
                &workspace_version,
                local_manifest_path,
            )?;

            // Apply the SAME version and SAME changelog to ALL packages
            for (p, diff) in packages_diffs {
                debug!(
                    "package: {}, unified version: {workspace_version}",
                    p.name,
                );

                // For unified versioning, all packages get the same changelog
                let update_result = UpdateResult {
                    version: workspace_version.clone(),
                    changelog: workspace_changelog.0.clone(),
                    semver_check: diff.semver_check,
                    new_changelog_entry: workspace_changelog.1.clone(),
                };

                packages_to_update
                    .updates_mut()
                    .push((p.clone(), update_result));
            }
        } else {
            info!("no commits since last tag - no updates needed");
        }

        Ok(packages_to_update)
    }


    /// Calculate the unified workspace version based on ALL commits from ALL packages.
    /// This is the core of unified workspace versioning - one version for entire monorepo.
    fn calculate_unified_workspace_version(
        &self,
        local_manifest_path: &Utf8Path,
        all_commits: &[Commit],
    ) -> anyhow::Result<Version> {
        let local_manifest = LocalManifest::try_new(local_manifest_path)?;

        // Try to get workspace version first, fallback to package version for single-package projects
        let current_workspace_version = if let Some(version) = local_manifest.get_workspace_version() {
            version
        } else if let Some(version) = local_manifest.get_package_version() {
            version
        } else {
            anyhow::bail!("Could not find version in Cargo.toml. For workspaces, set workspace.package.version. For single packages, set package.version.");
        };

        // Use default version updater configuration
        // TODO: Make this configurable via workspace config
        let version_updater = VersionUpdater::default();

        // Calculate next version based on ALL commits
        let next_version = if all_commits.is_empty() {
            // No commits, keep current version
            current_workspace_version
        } else {
            // Analyze commits to determine version bump
            version_updater.increment(&current_workspace_version, all_commits.iter().map(|c| &c.message))
        };

        Ok(next_version)
    }

    /// Generate a single workspace changelog for the entire monorepo.
    /// Returns (full_changelog, new_entry_only)
    fn generate_workspace_changelog(
        &self,
        all_commits: &[Commit],
        workspace_version: &Version,
        local_manifest_path: &Utf8Path,
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        // Get workspace-level changelog path (defaults to ./CHANGELOG.md at workspace root)
        let workspace_changelog_path = local_manifest_path
            .parent()
            .unwrap()
            .join("CHANGELOG.md");

        // Read existing changelog if it exists
        let old_changelog = if workspace_changelog_path.exists() {
            Some(std::fs::read_to_string(&workspace_changelog_path)?)
        } else {
            None
        };

        // Get current workspace version for comparison
        let local_manifest = LocalManifest::try_new(local_manifest_path)?;
        let current_version = if let Some(version) = local_manifest.get_workspace_version() {
            version
        } else if let Some(version) = local_manifest.get_package_version() {
            version
        } else {
            anyhow::bail!("Could not find version in Cargo.toml");
        };

        // Generate changelog using workspace context
        let repo_url = self.req.repo_url();
        let release_link = {
            let prev_tag = self.project.git_tag(&current_version.to_string())?;
            let next_tag = self.project.git_tag(&workspace_version.to_string())?;
            repo_url.map(|r| r.git_release_link(&prev_tag, &next_tag))
        };

        let changelog_req = self.req.changelog_req().clone();

        // Use "workspace" as the package name for unified changelog
        let (full_changelog, new_entry) = get_workspace_changelog(
            all_commits,
            workspace_version,
            Some(changelog_req),
            old_changelog.as_deref(),
            repo_url,
            release_link.as_deref(),
            &current_version,
        )?;

        Ok((Some(full_changelog), Some(new_entry)))
    }

    async fn get_packages_diffs(
        &self,
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
                    .get_diff(p, repository)
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

        let semver_check_result: anyhow::Result<()> =
            packages_diffs.par_iter_mut().try_for_each(|(p, diff)| {
                let package_config = self.req.get_package_config(&p.name);
                for pkg_to_include in &package_config.changelog_include {
                    if let Some(commits) = packages_commits.get(pkg_to_include) {
                        diff.add_commits(commits);
                    }
                }
                // TODO: Implement git-tag-based semver checking
                // For now, semver checking is skipped when using git tags only.
                // It can be implemented by checking out the tag commit and comparing packages.
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



    /// This operation is not thread-safe, because we do `git checkout` on the repository.
    #[instrument(
        skip_all,
        fields(package = %package.name)
    )]
    fn get_diff(
        &self,
        package: &Package,
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

        let git_tag = self
            .project
            .git_tag(&package.version.to_string())?;
        let tag_commit = repository.get_tag_commit(&git_tag);
        let tag_exists = tag_commit.is_some();

        let mut diff = Diff::new(tag_exists);
        let pathbufs_to_check = pathbufs_to_check(&package_path, package)?;
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

        self.get_package_diff(
            &package_path,
            package,
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
        repository: &Repo,
        tag_commit: Option<&str>,
        diff: &mut Diff,
    ) -> anyhow::Result<()> {
        let pathbufs_to_check = pathbufs_to_check(package_path, package)?;
        let paths_to_check: Vec<&Path> = pathbufs_to_check.iter().map(|p| p.as_ref()).collect();

        // If no tag exists (first release), limit the commits we analyze
        let max_analyze_commits = if tag_commit.is_none() {
            match self.req.max_analyze_commits() {
                0 => u32::MAX,
                n => n,
            }
        } else {
            u32::MAX
        };

        for _ in 0..max_analyze_commits {
            let current_commit_message = repository.current_commit_message()?;
            let current_commit_hash = repository.current_commit_hash()?;

            // Check if files changed in git commit belong to the current package.
            // This is required because a package can contain another package in a subdirectory.
            let are_changed_files_in_pkg = || {
                self.are_changed_files_in_package(package_path, repository, &current_commit_hash)
            };

            // If we reached the tag commit, stop here
            if is_commit_too_old(
                repository,
                tag_commit,
                None, // No longer using published_at_commit from registry
                &current_commit_hash,
            ) {
                debug!(
                    "next version calculated starting from commits after `{current_commit_hash}`"
                );
                break;
            }

            // Add commit if files in this package changed
            if are_changed_files_in_pkg()? {
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
    let repository_dir = repository.directory();

    crate::get_cargo_package_files(package_path)?
        .into_iter()
        // filter file generated by `cargo package` that isn't in git.
        .filter(|file| file != "Cargo.toml.orig" && file != ".cargo_vcs_info.json")
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

/// Check if commit belongs to a previous version of the package.
/// `tag_commit` is the commit hash of the tag of the previous version.
/// `published_at_commit` is the commit hash where `cargo publish` ran.
fn is_commit_too_old(
    repository: &Repo,
    tag_commit: Option<&str>,
    published_at_commit: Option<&str>,
    current_commit_hash: &str,
) -> bool {
    if let Some(tag_commit) = tag_commit.as_ref()
        && repository.is_ancestor(current_commit_hash, tag_commit)
    {
        debug!(
            "stopping looking at git history because the current commit ({}) is an ancestor of the commit ({}) tagged with the previous version.",
            current_commit_hash, tag_commit
        );
        return true;
    }

    if let Some(published_commit) = published_at_commit.as_ref()
        && repository.is_ancestor(current_commit_hash, published_commit)
    {
        debug!(
            "stopping looking at git history because the current commit ({}) is an ancestor of the commit ({}) where the previous version was published.",
            current_commit_hash, published_commit
        );
        return true;
    }
    false
}

fn pathbufs_to_check(
    package_path: &Utf8Path,
    package: &Package,
) -> anyhow::Result<Vec<Utf8PathBuf>> {
    let mut paths = vec![package_path.to_path_buf()];
    if let Some(readme_path) = crate::local_readme_override(package, package_path)? {
        paths.push(readme_path);
    }
    Ok(paths)
}

/// Generate a workspace-level changelog (for unified monorepo versioning).
/// Returns (full_changelog, new_entry_only)
fn get_workspace_changelog(
    commits: &[Commit],
    next_version: &Version,
    changelog_req: Option<ChangelogRequest>,
    old_changelog: Option<&str>,
    repo_url: Option<&RepoUrl>,
    release_link: Option<&str>,
    current_version: &Version,
) -> anyhow::Result<(String, String)> {
    let commits: Vec<git_cliff_core::commit::Commit> =
        commits.iter().map(|c| c.to_cliff_commit()).collect();

    // Use "workspace" as the package name for unified changelog
    let mut changelog_builder = ChangelogBuilder::new(
        commits.clone(),
        next_version.to_string(),
        "workspace".to_string(),
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

        let is_new_version = next_version != current_version;
        let last_version = old_changelog.and_then(|old_changelog| {
            changelog_parser::last_version_from_str(old_changelog)
                .ok()
                .flatten()
        });

        if is_new_version {
            let last_version = last_version.unwrap_or(current_version.to_string());
            changelog_builder = changelog_builder.with_previous_version(last_version);
        } else if let Some(last_version) = last_version
            && let Some(old_changelog) = old_changelog
            && last_version == next_version.to_string()
        {
            // If the next version is the same as the last version of the changelog,
            // don't update the changelog (returning the old one).
            return Ok((old_changelog.to_string(), String::new()));
        }
    }

    let new_changelog = changelog_builder.build();
    let changelog = match old_changelog {
        Some(old_changelog) => new_changelog.prepend(old_changelog)?,
        None => new_changelog.generate()?,
    };
    let body_only =
        new_changelog_entry(changelog_builder).context("can't determine changelog body")?;
    Ok((changelog, body_only.unwrap_or_default()))
}

/// Return the following tuple:
/// - the entire changelog (with the new entries);
/// - the new changelog entry alone
///   (i.e. changelog body update without header and footer).
fn get_changelog(
    commits: &[Commit],
    next_version: &Version,
    changelog_req: Option<ChangelogRequest>,
    old_changelog: Option<&str>,
    repo_url: Option<&RepoUrl>,
    release_link: Option<&str>,
    package: &Package,
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
        } else if let Some(last_version) = last_version
            && let Some(old_changelog) = old_changelog
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
    let changelog = match old_changelog {
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

        let old = r"## [1.1.0] - 1970-01-01

### fix bugs
- my awesomefix

### other
- complex update
";
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
        assert_eq!(old, new.0);
    }
}
