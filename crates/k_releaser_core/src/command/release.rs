use std::collections::{BTreeMap, HashSet};

use anyhow::Context;
use cargo::util::VersionExt;
use cargo_metadata::{Metadata, Package, camino::Utf8PathBuf, semver::Version};
use git_cmd::Repo;
use serde::Serialize;
use tracing::{debug, info, instrument, trace, warn};

use crate::{
    CHANGELOG_FILENAME, DEFAULT_BRANCH_PREFIX, GitForge, PackagePath, Project, ReleaseMetadata,
    ReleaseMetadataBuilder, Remote, changelog_parser,
    git::forge::GitClient,
    pr_parser::{Pr, prs_from_text},
};

#[derive(Debug)]
pub struct ReleaseRequest {
    /// Cargo metadata.
    metadata: Metadata,
    /// Perform all checks without creating tags/releases.
    dry_run: bool,
    /// If true, release on every commit.
    /// If false, release only on Release PR merge.
    release_always: bool,
    /// Publishes GitHub release.
    git_release: Option<GitRelease>,
    /// GitHub/Gitea/Gitlab repository url where your project is hosted.
    /// It is used to create the git release.
    /// It defaults to the url of the default remote.
    repo_url: Option<String>,
    /// Package-specific configurations.
    packages_config: PackagesConfig,
    /// PR Branch Prefix
    branch_prefix: String,
}

impl ReleaseRequest {
    pub fn new(metadata: Metadata) -> Self {
        Self {
            metadata,
            dry_run: false,
            git_release: None,
            repo_url: None,
            packages_config: PackagesConfig::default(),
            release_always: true,
            branch_prefix: DEFAULT_BRANCH_PREFIX.to_string(),
        }
    }

    /// The manifest of the project you want to release.
    pub fn local_manifest(&self) -> Utf8PathBuf {
        cargo_utils::workspace_manifest(&self.metadata)
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_git_release(mut self, git_release: GitRelease) -> Self {
        self.git_release = Some(git_release);
        self
    }

    pub fn with_repo_url(mut self, repo_url: impl Into<String>) -> Self {
        self.repo_url = Some(repo_url.into());
        self
    }

    pub fn with_default_package_config(mut self, config: ReleaseConfig) -> Self {
        self.packages_config.set_default(config);
        self
    }

    pub fn with_release_always(mut self, release_always: bool) -> Self {
        self.release_always = release_always;
        self
    }

    pub fn with_branch_prefix(mut self, pr_branch_prefix: Option<String>) -> Self {
        if let Some(branch_prefix) = pr_branch_prefix {
            self.branch_prefix = branch_prefix;
        }
        self
    }

    /// Set release config for a specific package.
    pub fn with_package_config(
        mut self,
        package: impl Into<String>,
        config: ReleaseConfig,
    ) -> Self {
        self.packages_config.set(package.into(), config);
        self
    }

    pub fn changelog_path(&self, package: &Package) -> Utf8PathBuf {
        let config = self.get_package_config(&package.name);
        config
            .changelog_path
            .map(|p| self.metadata.workspace_root.join(p))
            .unwrap_or_else(|| {
                package
                    .package_path()
                    .expect("can't determine package path")
                    .join(CHANGELOG_FILENAME)
            })
    }

    fn is_git_release_enabled(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.git_release.enabled
    }

    fn is_git_tag_enabled(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.git_tag.enabled
    }

    pub fn get_package_config(&self, package: &str) -> ReleaseConfig {
        self.packages_config.get(package)
    }
}

impl ReleaseMetadataBuilder for ReleaseRequest {
    fn get_release_metadata(&self, package_name: &str) -> Option<ReleaseMetadata> {
        let config = self.get_package_config(package_name);
        Some(ReleaseMetadata {
            tag_name_template: config.git_tag.name_template.clone(),
            release_name_template: config.git_release.name_template.clone(),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PackagesConfig {
    /// Config for packages that don't have a specific configuration.
    default: ReleaseConfig,
    /// Configurations that override `default`.
    /// The key is the package name.
    overrides: BTreeMap<String, ReleaseConfig>,
}

impl PackagesConfig {
    fn get(&self, package_name: &str) -> ReleaseConfig {
        self.overrides
            .get(package_name)
            .cloned()
            .unwrap_or(self.default.clone())
    }

    fn set_default(&mut self, config: ReleaseConfig) {
        self.default = config;
    }

    fn set(&mut self, package_name: String, config: ReleaseConfig) {
        self.overrides.insert(package_name, config);
    }

    pub fn overridden_packages(&self) -> HashSet<&str> {
        self.overrides.keys().map(|s| s.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseConfig {
    publish: PublishConfig,
    git_release: GitReleaseConfig,
    git_tag: GitTagConfig,
    /// Don't verify the contents by building them.
    /// If true, `release-plz` adds the `--no-verify` flag to `cargo publish`.
    no_verify: bool,
    /// Allow dirty working directories to be packaged.
    /// If true, `release-plz` adds the `--allow-dirty` flag to `cargo publish`.
    allow_dirty: bool,
    /// Features to be enabled when packaging the crate.
    /// If non-empty, pass the `--features` flag to `cargo publish`.
    features: Vec<String>,
    /// Enable all features when packaging the crate.
    /// If true, pass the `--all-features` flag to `cargo publish`.
    all_features: bool,
    changelog_path: Option<Utf8PathBuf>,
    /// Whether this package has a changelog that release-plz updates or not.
    /// Default: `true`.
    changelog_update: bool,
}

impl ReleaseConfig {
    pub fn with_publish(mut self, publish: PublishConfig) -> Self {
        self.publish = publish;
        self
    }

    pub fn with_git_release(mut self, git_release: GitReleaseConfig) -> Self {
        self.git_release = git_release;
        self
    }

    pub fn with_git_tag(mut self, git_tag: GitTagConfig) -> Self {
        self.git_tag = git_tag;
        self
    }

    pub fn with_no_verify(mut self, no_verify: bool) -> Self {
        self.no_verify = no_verify;
        self
    }

    pub fn with_allow_dirty(mut self, allow_dirty: bool) -> Self {
        self.allow_dirty = allow_dirty;
        self
    }

    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }

    pub fn with_all_features(mut self, all_features: bool) -> Self {
        self.all_features = all_features;
        self
    }

    pub fn with_changelog_path(mut self, changelog_path: Utf8PathBuf) -> Self {
        self.changelog_path = Some(changelog_path);
        self
    }

    pub fn with_changelog_update(mut self, changelog_update: bool) -> Self {
        self.changelog_update = changelog_update;
        self
    }

    pub fn publish(&self) -> &PublishConfig {
        &self.publish
    }

    pub fn git_release(&self) -> &GitReleaseConfig {
        &self.git_release
    }
}

impl Default for ReleaseConfig {
    fn default() -> Self {
        Self {
            publish: PublishConfig::default(),
            git_release: GitReleaseConfig::default(),
            git_tag: GitTagConfig::default(),
            no_verify: false,
            allow_dirty: false,
            features: vec![],
            all_features: false,
            changelog_path: None,
            changelog_update: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishConfig {
    enabled: bool,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self::enabled(true)
    }
}

impl PublishConfig {
    pub fn enabled(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ReleaseType {
    #[default]
    Prod,
    Pre,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitReleaseConfig {
    enabled: bool,
    draft: bool,
    latest: Option<bool>,
    release_type: ReleaseType,
    name_template: Option<String>,
    body_template: Option<String>,
}

impl Default for GitReleaseConfig {
    fn default() -> Self {
        Self::enabled(true)
    }
}

impl GitReleaseConfig {
    pub fn enabled(enabled: bool) -> Self {
        Self {
            enabled,
            draft: false,
            latest: None,
            release_type: ReleaseType::default(),
            name_template: None,
            body_template: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_draft(mut self, draft: bool) -> Self {
        self.draft = draft;
        self
    }

    pub fn set_latest(mut self, latest: bool) -> Self {
        self.latest = Some(latest);
        self
    }

    pub fn set_release_type(mut self, release_type: ReleaseType) -> Self {
        self.release_type = release_type;
        self
    }

    pub fn set_name_template(mut self, name_template: Option<String>) -> Self {
        self.name_template = name_template;
        self
    }

    pub fn set_body_template(mut self, body_template: Option<String>) -> Self {
        self.body_template = body_template;
        self
    }

    pub fn is_pre_release(&self, version: &Version) -> bool {
        match self.release_type {
            ReleaseType::Pre => true,
            ReleaseType::Auto => version.is_prerelease(),
            ReleaseType::Prod => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTagConfig {
    enabled: bool,
    name_template: Option<String>,
}

impl Default for GitTagConfig {
    fn default() -> Self {
        Self::enabled(true)
    }
}

impl GitTagConfig {
    pub fn enabled(enabled: bool) -> Self {
        Self {
            enabled,
            name_template: None,
        }
    }

    pub fn set_name_template(mut self, name_template: Option<String>) -> Self {
        self.name_template = name_template;
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug)]
pub struct GitRelease {
    /// Kind of Git Forge.
    pub forge: GitForge,
}

#[derive(Serialize, Default, Debug)]
pub struct Release {
    releases: Vec<PackageRelease>,
}

#[derive(Serialize, Debug)]
pub struct PackageRelease {
    package_name: String,
    prs: Vec<Pr>,
    /// Git tag name. It's not guaranteed that release-plz created the git tag.
    /// In fact, users can disable git tag creation in the [`ReleaseRequest`].
    /// We return the git tag name anyway, because users might use it to create
    /// the tag by themselves.
    tag: String,
    version: Version,
}

/// Release the project as it is.
#[instrument(skip(input))]
pub async fn release(input: &ReleaseRequest) -> anyhow::Result<Option<Release>> {
    let overrides = input.packages_config.overridden_packages();
    let project = Project::new(
        &input.local_manifest(),
        None,
        &overrides,
        &input.metadata,
        input,
    )?;
    let repo = Repo::new(&input.metadata.workspace_root)?;

    // Fetch tags from remote to ensure we have the latest tag information
    // This prevents attempting to create duplicate tags
    if let Err(e) = repo.git(&["fetch", "--tags"]) {
        debug!("Failed to fetch tags (this is ok if there's no remote): {e}");
    }

    let git_client = get_git_client(input)?;
    let should_release = should_release(input, &repo, &git_client).await?;
    debug!("should release: {should_release:?}");

    if should_release == ShouldRelease::No {
        debug!("skipping release");
        return Ok(None);
    }

    let mut checkout_done = false;
    if let ShouldRelease::YesWithCommit(commit) = &should_release {
        match repo.checkout(commit) {
            Ok(()) => {
                debug!("checking out commit {commit}");
                checkout_done = true;
            }
            // The commit does not exist if the PR was squashed.
            Err(_) => trace!("checkout failed; continuing"),
        }
    }

    // Don't return the error immediately because we want to go back to the previous commit if needed
    let release = release_packages(input, &project, &repo, &git_client).await;

    if let ShouldRelease::YesWithCommit(_) = should_release {
        // Go back to the previous commit so that the user finds
        // the repository in the same commit they launched release-plz.
        if checkout_done {
            repo.checkout("-")?;
            trace!("restored previous commit after release");
        }
    }

    release
}

async fn release_packages(
    input: &ReleaseRequest,
    project: &Project,
    repo: &Repo,
    git_client: &GitClient,
) -> anyhow::Result<Option<Release>> {
    // Packages are already ordered by release order.
    let packages = project.publishable_packages();
    if packages.is_empty() {
        info!("nothing to release");
        return Ok(None);
    }

    // Check if all packages have the same version (unified workspace versioning)
    let first_version = &packages[0].version;
    let is_unified_workspace = packages.iter().all(|p| &p.version == first_version);

    if is_unified_workspace && packages.len() > 1 {
        // Unified workspace versioning: create ONE release for the workspace
        info!("Detected unified workspace versioning - creating single workspace release");
        release_unified_workspace(input, project, &packages, repo, git_client).await
    } else {
        // Multi-package versioning: release each package individually
        let mut package_releases: Vec<PackageRelease> = vec![];
        for package in packages {
            if let Some(pkg_release) =
                release_package_if_needed(input, project, package, repo, git_client).await?
            {
                package_releases.push(pkg_release);
            }
        }
        let release = (!package_releases.is_empty()).then_some(Release {
            releases: package_releases,
        });
        Ok(release)
    }
}

/// Get changelog entry for unified workspace release from the release PR body.
/// The PR body is generated from git history (the source of truth) and can be reviewed/modified by users.
async fn get_workspace_changelog_entry(
    input: &ReleaseRequest,
    repo: &Repo,
    git_client: &GitClient,
) -> anyhow::Result<String> {
    // Get the release PR associated with the current commit
    let last_commit = repo.current_commit_hash()?;
    let prs = git_client.associated_prs(&last_commit).await?;
    let release_pr = prs
        .iter()
        .find(|pr| pr.branch().starts_with(&input.branch_prefix));

    if let Some(pr) = release_pr {
        if let Some(body) = &pr.body {
            // Extract changelog from PR body
            // The PR body contains the changelog generated from git history
            let changelog = extract_changelog_from_pr_body(body);
            debug!("Using changelog from release PR #{}", pr.number);
            return Ok(changelog);
        }
    }

    warn!("No release PR found or PR has no body. Release will have empty body.");
    Ok(String::new())
}

/// Extract changelog content from release PR body.
/// The PR body has changelog in <details><summary>Changelog</summary>...</details>
fn extract_changelog_from_pr_body(pr_body: &str) -> String {
    // Look for content between <details> tags
    if let Some(start) = pr_body.find("<details>") {
        if let Some(end) = pr_body[start..].find("</details>") {
            let details_content = &pr_body[start..start + end];
            // Skip the <details> and <summary> tags to get just the changelog content
            if let Some(summary_end) = details_content.find("</summary>") {
                let changelog = &details_content[summary_end + "</summary>".len()..];
                return changelog.trim().to_string();
            }
        }
    }

    // If we can't find the details tag, return the whole body
    // (useful if the format changes or for custom PR bodies)
    pr_body.to_string()
}

/// Release a unified workspace with a single version for all packages
async fn release_unified_workspace(
    input: &ReleaseRequest,
    project: &Project,
    packages: &[&Package],
    repo: &Repo,
    git_client: &GitClient,
) -> anyhow::Result<Option<Release>> {
    let version = &packages[0].version;
    let git_tag = project.git_tag(&version.to_string())?;

    // Check if tag already exists
    if repo.tag_exists(&git_tag)? {
        info!("Tag {} already exists - skipping release", git_tag);
        return Ok(None);
    }

    // Try to get changelog from CHANGELOG.md first, then fall back to release PR body
    let changelog_entry = get_workspace_changelog_entry(input, repo, git_client).await?;

    // Extract PRs from changelog if present
    let prs = prs_from_text(&changelog_entry);

    // For unified workspace, check if there's a custom release name template
    // If yes, use "workspace" as the package name; if no, use "Version {version}" format
    let release_config = input.get_package_config(&packages[0].name);
    let release_name = if release_config.git_release.name_template.is_some() {
        // Use custom template with "workspace" as package name
        project.release_name("workspace", &version.to_string())?
    } else {
        // Use default "Version X.Y.Z" format for unified workspace
        format!("Version {}", version)
    };

    let release_info = ReleaseInfo {
        package: packages[0], // Use first package for metadata
        git_tag: &git_tag,
        release_name: &release_name,
        changelog: &changelog_entry,
        prs: &prs,
    };

    let was_released = release_package(input, repo, git_client, &release_info).await?;

    if was_released {
        let package_names: Vec<String> = packages.iter().map(|p| p.name.to_string()).collect();
        info!("Released workspace version {} for packages: {}", version, package_names.join(", "));

        // Return a single PackageRelease representing the unified workspace
        Ok(Some(Release {
            releases: vec![PackageRelease {
                package_name: "workspace".to_string(),
                prs,
                tag: git_tag,
                version: version.clone(),
            }],
        }))
    } else {
        Ok(None)
    }
}

async fn release_package_if_needed(
    input: &ReleaseRequest,
    project: &Project,
    package: &Package,
    repo: &Repo,
    git_client: &GitClient,
) -> anyhow::Result<Option<PackageRelease>> {
    let git_tag = project.git_tag(&package.version.to_string())?;
    let release_name = project.release_name(&package.name, &package.version.to_string())?;
    if repo.tag_exists(&git_tag)? {
        info!(
            "{} {}: Already released - Tag {} already exists",
            package.name, package.version, &git_tag
        );
        return Ok(None);
    }

    let changelog = last_changelog_entry(input, package);
    let prs = prs_from_text(&changelog);
    let release_info = ReleaseInfo {
        package,
        git_tag: &git_tag,
        release_name: &release_name,
        changelog: &changelog,
        prs: &prs,
    };

    let package_was_released = release_package(input, repo, git_client, &release_info)
        .await
        .context("failed to release package")?;

    let package_release = package_was_released.then_some(PackageRelease {
        package_name: package.name.to_string(),
        version: package.version.clone(),
        tag: git_tag,
        prs,
    });
    Ok(package_release)
}

#[derive(Debug, PartialEq, Eq)]
enum ShouldRelease {
    Yes,
    YesWithCommit(String),
    No,
}

async fn should_release(
    input: &ReleaseRequest,
    repo: &Repo,
    git_client: &GitClient,
) -> anyhow::Result<ShouldRelease> {
    let last_commit = repo.current_commit_hash()?;
    let prs = git_client.associated_prs(&last_commit).await?;
    let associated_release_pr = prs
        .iter()
        .find(|pr| pr.branch().starts_with(&input.branch_prefix));

    match associated_release_pr {
        Some(pr) => {
            let pr_commits = git_client.pr_commits(pr.number).await?;
            // Get the last commit of the PR, i.e. the last commit that was pushed before the PR was merged
            match pr_commits.last() {
                Some(commit) if commit.sha != last_commit => {
                    if is_pr_commit_in_original_branch(repo, commit) {
                        // I need to checkout the last commit of the PR if it exists
                        Ok(ShouldRelease::YesWithCommit(commit.sha.clone()))
                    } else {
                        // The commit is not in the original branch, probably the PR was squashed
                        Ok(ShouldRelease::Yes)
                    }
                }
                _ => {
                    // I'm already at the right commit
                    Ok(ShouldRelease::Yes)
                }
            }
        }
        None => {
            if input.release_always {
                Ok(ShouldRelease::Yes)
            } else {
                info!("skipping release: current commit is not from a release PR");
                Ok(ShouldRelease::No)
            }
        }
    }
}

fn is_pr_commit_in_original_branch(repo: &Repo, commit: &crate::git::forge::PrCommit) -> bool {
    let branches_of_commit = repo.get_branches_of_commit(&commit.sha);
    if let Ok(branches) = branches_of_commit {
        branches.contains(&repo.original_branch().to_string())
    } else {
        false
    }
}

struct ReleaseInfo<'a> {
    package: &'a Package,
    git_tag: &'a str,
    release_name: &'a str,
    changelog: &'a str,
    prs: &'a [Pr],
}

/// Return `true` if package was released, `false` otherwise.
async fn release_package(
    input: &ReleaseRequest,
    repo: &Repo,
    git_client: &GitClient,
    release_info: &ReleaseInfo<'_>,
) -> anyhow::Result<bool> {
    let should_create_git_tag = input.is_git_tag_enabled(&release_info.package.name);
    let should_create_git_release = input.is_git_release_enabled(&release_info.package.name);

    if input.dry_run {
        log_dry_run_info(
            release_info,
            should_create_git_tag,
            should_create_git_release,
        );
        Ok(false)
    } else {
        if should_create_git_tag {
            // Use same tag message of cargo-release
            let message = format!(
                "chore: Release package {} version {}",
                release_info.package.name, release_info.package.version
            );
            let should_sign_tags = repo
                .git(&["config", "--default", "false", "--get", "tag.gpgSign"])
                .map(|s| s.trim() == "true")?;
            // If tag signing is enabled, create the tag locally instead of using the API
            if should_sign_tags {
                repo.tag(release_info.git_tag, &message)?;
                repo.push(release_info.git_tag)?;
            } else {
                let sha = repo.current_commit_hash()?;
                git_client
                    .create_tag(release_info.git_tag, &message, &sha)
                    .await?;
            }
        }

        let contributors = get_contributors(release_info, git_client).await;

        // TODO fill the rest
        let remote = Remote {
            owner: String::new(),
            repo: String::new(),
            link: String::new(),
            contributors,
        };
        if should_create_git_release {
            let release_body =
                release_body(input, release_info.package, release_info.changelog, &remote);
            let release_config = input
                .get_package_config(&release_info.package.name)
                .git_release;
            let is_pre_release = release_config.is_pre_release(&release_info.package.version);
            let git_release_info = GitReleaseInfo {
                git_tag: release_info.git_tag.to_string(),
                release_name: release_info.release_name.to_string(),
                release_body,
                draft: release_config.draft,
                latest: release_config.latest,
                pre_release: is_pre_release,
            };
            git_client.create_release(&git_release_info).await?;
        }

        info!(
            "released {} {}",
            release_info.package.name, release_info.package.version
        );
        Ok(true)
    }
}

/// Traces the steps that would have been taken had release been run without dry-run.
fn log_dry_run_info(
    release_info: &ReleaseInfo,
    should_create_git_tag: bool,
    should_create_git_release: bool,
) {
    let prefix = format!(
        "{} {}:",
        release_info.package.name, release_info.package.version
    );

    let mut items_to_skip = vec![];

    if should_create_git_tag {
        items_to_skip.push(format!("creation of tag '{}'", release_info.git_tag));
    }

    if should_create_git_release {
        items_to_skip.push("creation of git release".to_string());
    }

    if items_to_skip.is_empty() {
        info!("{prefix} no release method enabled");
    } else {
        info!("{prefix} due to dry run, skipping: {items_to_skip:?}");
    }
}

async fn get_contributors(
    release_info: &ReleaseInfo<'_>,
    git_client: &GitClient,
) -> Vec<git_cliff_core::contributor::RemoteContributor> {
    let prs_number = release_info
        .prs
        .iter()
        .map(|pr| pr.number)
        .collect::<Vec<_>>();

    let mut unique_usernames = std::collections::HashSet::new();

    git_client
        .get_prs_info(&prs_number)
        .await
        .inspect_err(|e| tracing::warn!("failed to retrieve contributors: {e}"))
        .unwrap_or(vec![])
        .iter()
        .filter_map(|pr| {
            let username = &pr.user.login;
            // Only include this contributor if we haven't seen their username before
            unique_usernames.insert(username).then(|| {
                git_cliff_core::contributor::RemoteContributor {
                    username: Some(username.clone()),
                    ..Default::default()
                }
            })
        })
        .collect()
}

fn get_git_client(input: &ReleaseRequest) -> anyhow::Result<GitClient> {
    let git_release = input
        .git_release
        .as_ref()
        .context("git release not configured. Did you specify git-token and forge?")?;
    GitClient::new(git_release.forge.clone())
}

#[derive(Debug)]
pub struct GitReleaseInfo {
    pub git_tag: String,
    pub release_name: String,
    pub release_body: String,
    pub latest: Option<bool>,
    pub draft: bool,
    pub pre_release: bool,
}

/// Return an empty string if the changelog cannot be parsed.
fn release_body(
    req: &ReleaseRequest,
    package: &Package,
    changelog: &str,
    remote: &Remote,
) -> String {
    let body_template = req
        .get_package_config(&package.name)
        .git_release
        .body_template;
    crate::tera::release_body_from_template(
        &package.name,
        &package.version.to_string(),
        changelog,
        remote,
        body_template.as_deref(),
    )
    .unwrap_or_else(|e| {
        warn!(
            "{}: failed to generate release body: {:?}. The git release body will be empty.",
            package.name, e
        );
        String::new()
    })
}

/// Return an empty string if not found.
fn last_changelog_entry(req: &ReleaseRequest, package: &Package) -> String {
    let changelog_update = req.get_package_config(&package.name).changelog_update;
    if !changelog_update {
        return String::new();
    }
    let changelog_path = req.changelog_path(package);
    match changelog_parser::last_changes(&changelog_path) {
        Ok(Some(changes)) => changes,
        Ok(None) => {
            warn!(
                "{}: last change not found in changelog at path {:?}. The git release body will be empty.",
                package.name, &changelog_path
            );
            String::new()
        }
        Err(e) => {
            warn!(
                "{}: failed to parse changelog at path {:?}: {:?}. The git release body will be empty.",
                package.name, &changelog_path, e
            );
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_changelog_from_pr_body() {
        let pr_body = r#"
## 🤖 New release v0.1.1

This release updates all workspace packages to version **0.1.1**.

### Packages updated

* `my-package`


<details><summary><i><b>Changelog</b></i></summary>

### Fixed

- add config file
- cargo init

### Other

- Initial commit

</details>



---
🤖 Generated by k-releaser"#;

        let changelog = extract_changelog_from_pr_body(pr_body);
        assert!(changelog.contains("### Fixed"));
        assert!(changelog.contains("- add config file"));
        assert!(changelog.contains("- cargo init"));
        assert!(changelog.contains("### Other"));
        assert!(changelog.contains("- Initial commit"));
        assert!(!changelog.contains("<details>"));
        assert!(!changelog.contains("</details>"));
        assert!(!changelog.contains("Generated by k-releaser"));
    }

    #[test]
    fn test_extract_changelog_from_pr_body_without_details_tag() {
        let pr_body = "Some custom PR body without details tag";
        let changelog = extract_changelog_from_pr_body(pr_body);
        assert_eq!(changelog, pr_body);
    }

    #[test]
    fn git_release_config_pre_release_default_works() {
        let config = GitReleaseConfig::default();
        let version = Version::parse("1.0.0").unwrap();
        let rc_version = Version::parse("1.0.0-rc1").unwrap();

        assert!(!config.is_pre_release(&version));
        assert!(!config.is_pre_release(&rc_version));
    }

    #[test]
    fn git_release_config_pre_release_auto_works() {
        let mut config = GitReleaseConfig::default();
        config = config.set_release_type(ReleaseType::Auto);
        let version = Version::parse("1.0.0").unwrap();
        let rc_version = Version::parse("1.0.0-rc1").unwrap();

        assert!(!config.is_pre_release(&version));
        assert!(config.is_pre_release(&rc_version));
    }

    #[test]
    fn git_release_config_pre_release_pre_works() {
        let mut config = GitReleaseConfig::default();
        config = config.set_release_type(ReleaseType::Pre);
        let version = Version::parse("1.0.0").unwrap();
        let rc_version = Version::parse("1.0.0-rc1").unwrap();

        assert!(config.is_pre_release(&version));
        assert!(config.is_pre_release(&rc_version));
    }
}
