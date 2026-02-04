use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};

use crate::command::trusted_publishing;
use anyhow::Context;
use cargo::util::VersionExt;
use cargo_metadata::{
    Metadata, Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use crates_index::{GitIndex, SparseIndex};
use git_cmd::Repo;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use tracing::{debug, info, instrument, trace, warn};
use url::Url;

use crate::{
    CHANGELOG_FILENAME, DEFAULT_BRANCH_PREFIX, GitForge, PackagePath, Project, Publishable as _,
    ReleaseMetadata, ReleaseMetadataBuilder, Remote,
    cargo::{CargoIndex, CargoRegistry, CmdOutput, is_published, run_cargo, wait_until_published},
    cargo_hash_kind::{get_hash_kind, try_get_fallback_hash_kind},
    changelog_parser,
    git::forge::GitClient,
    pr_parser::{Pr, prs_from_text},
};

#[derive(Debug)]
pub struct ReleaseRequest {
    /// Cargo metadata.
    metadata: Metadata,
    /// Registry where you want to publish the packages.
    /// The registry name needs to be present in the Cargo config.
    /// If unspecified, the `publish` field of the package manifest is used.
    /// If the `publish` field is empty, crates.io is used.
    registry: Option<String>,
    /// Token used to publish to the cargo registry.
    token: Option<SecretString>,
    /// Perform all checks without uploading.
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
    /// publish timeout
    publish_timeout: Duration,
    /// PR Branch Prefix
    branch_prefix: String,
}

impl ReleaseRequest {
    pub fn new(metadata: Metadata) -> Self {
        let minutes_30 = Duration::from_secs(30 * 60);
        Self {
            metadata,
            registry: None,
            token: None,
            dry_run: false,
            git_release: None,
            repo_url: None,
            packages_config: PackagesConfig::default(),
            publish_timeout: minutes_30,
            release_always: true,
            branch_prefix: DEFAULT_BRANCH_PREFIX.to_string(),
        }
    }

    /// The manifest of the project you want to release.
    pub fn local_manifest(&self) -> Utf8PathBuf {
        cargo_utils::workspace_manifest(&self.metadata)
    }

    pub fn with_registry(mut self, registry: impl Into<String>) -> Self {
        self.registry = Some(registry.into());
        self
    }

    pub fn with_token(mut self, token: impl Into<SecretString>) -> Self {
        self.token = Some(token.into());
        self
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

    pub fn with_publish_timeout(mut self, timeout: Duration) -> Self {
        self.publish_timeout = timeout;
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

    fn is_publish_enabled(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.publish.enabled
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

    pub fn allow_dirty(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.allow_dirty
    }

    pub fn no_verify(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.no_verify
    }

    pub fn features(&self, package: &str) -> Vec<String> {
        let config = self.get_package_config(package);
        config.features.clone()
    }

    pub fn all_features(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.all_features
    }

    pub fn registry_protocol(&self, package: &str) -> RegistryProtocol {
        let config = self.get_package_config(package);
        config.registry_protocol
    }

    /// Find the token to use for the given `registry` ([`Option::None`] means crates.io).
    fn find_registry_token(&self, registry: Option<&str>) -> anyhow::Result<Option<SecretString>> {
        let is_registry_same_as_request = self.registry.as_deref() == registry;
        let token = is_registry_same_as_request
            .then(|| self.token.clone())
            .flatten()
            // if the registry is not the same as the request or if there's no token in the request,
            // try to find the token in the Cargo credentials file or in the environment variables.
            .or(cargo_utils::registry_token(self.registry.as_deref())?);
        Ok(token)
    }

    /// Checks for inconsistency in the `publish` fields in the workspace metadata and release-plz config.
    ///
    /// If there is no inconsistency, returns Ok(())
    ///
    /// # Errors
    ///
    /// Errors if any package has `publish = false` or `publish = []` in the Cargo.toml
    /// but has `publish = true` in the release-plz configuration.
    pub fn check_publish_fields(&self) -> anyhow::Result<()> {
        let publish_fields = self.packages_config.publish_overrides_fields();

        for package in &self.metadata.packages {
            if !package.is_publishable()
                && let Some(should_publish) = publish_fields.get(package.name.as_str())
            {
                anyhow::ensure!(
                    !should_publish,
                    "Package `{}` has `publish = false` or `publish = []` in the Cargo.toml, but it has `publish = true` in the release-plz configuration.",
                    package.name
                );
            }
        }
        Ok(())
    }
}

impl ReleaseMetadataBuilder for ReleaseRequest {
    fn get_release_metadata(&self, package_name: &str) -> Option<ReleaseMetadata> {
        let config = self.get_package_config(package_name);
        config.release.then(|| ReleaseMetadata {
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

    // Return the `publish` fields explicitly set in the
    // `[[package]]` section of the release-plz config.
    // I.e. `publish` isn't inherited from the `[workspace]` section of the
    // release-plz config.
    pub fn publish_overrides_fields(&self) -> BTreeMap<String, bool> {
        self.overrides
            .iter()
            .map(|(package_name, release_config)| {
                (package_name.clone(), release_config.publish().is_enabled())
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseConfig {
    publish: PublishConfig,
    registry_protocol: RegistryProtocol,
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
    /// High-level toggle to process this package or ignore it
    release: bool,
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

    pub fn with_registry_protocol(mut self, registry_protocol: RegistryProtocol) -> Self {
        self.registry_protocol = registry_protocol;
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

    pub fn with_release(mut self, release: bool) -> Self {
        self.release = release;
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
            registry_protocol: RegistryProtocol::default(),
            git_release: GitReleaseConfig::default(),
            git_tag: GitTagConfig::default(),
            no_verify: false,
            allow_dirty: false,
            features: vec![],
            all_features: false,
            release: true,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegistryProtocol {
    #[default]
    Auto,
    Sparse,
    Git,
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
    }

    let mut package_releases: Vec<PackageRelease> = vec![];
    let hash_kind = get_hash_kind()?;
    // The same trusted publishing token can be used for all packages.
    let mut trusted_publishing_client: Option<trusted_publishing::TrustedPublisher> = None;
    for package in packages {
        if let Some(pkg_release) = release_package_if_needed(
            input,
            project,
            package,
            repo,
            git_client,
            &hash_kind,
            &mut trusted_publishing_client,
        )
        .await?
        {
            package_releases.push(pkg_release);
        }
    }
    if let Some(tp) = trusted_publishing_client.as_ref()
        && let Err(e) = tp.revoke_token().await
    {
        warn!("Failed to revoke trusted publishing token: {e:?}");
    }
    let release = (!package_releases.is_empty()).then_some(Release {
        releases: package_releases,
    });
    Ok(release)
}

async fn release_package_if_needed(
    input: &ReleaseRequest,
    project: &Project,
    package: &Package,
    repo: &Repo,
    git_client: &GitClient,
    hash_kind: &crates_index::HashKind,
    trusted_publishing_client: &mut Option<trusted_publishing::TrustedPublisher>,
) -> anyhow::Result<Option<PackageRelease>> {
    let git_tag = project.git_tag(&package.name, &package.version.to_string())?;
    let release_name = project.release_name(&package.name, &package.version.to_string())?;
    if repo.tag_exists(&git_tag)? {
        info!(
            "{} {}: Already published - Tag {} already exists",
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

    let should_publish = input.is_publish_enabled(&package.name);
    let mut package_was_released = false;

    if should_publish {
        let registry_indexes = registry_indexes(
            package,
            input.registry.clone(),
            input.registry_protocol(&package.name),
            hash_kind,
        )
        .context("can't determine registry indexes")?;

        for CargoRegistry {
            name,
            index: primary_index,
            fallback_index,
        } in registry_indexes
        {
            let token = input.find_registry_token(name.as_deref())?;
            let (pkg_is_published, mut index) =
                is_package_published(input, package, primary_index, fallback_index, &token)
                    .await
                    .with_context(|| {
                        format!("can't determine if package {} is published", package.name)
                    })?;

            if pkg_is_published {
                info!("{} {}: already published", package.name, package.version);
                continue;
            }
            let is_crates_io = name.is_none();
            let package_was_released_at_index = release_package(
                &mut index,
                input,
                repo,
                git_client,
                &release_info,
                &token,
                is_crates_io,
                trusted_publishing_client,
            )
            .await
            .context("failed to release package")?;

            if package_was_released_at_index {
                package_was_released = true;
            }
        }
    } else {
        // When publishing is disabled (e.g., git_only mode), skip registry checks entirely
        // and only perform git tag/release operations.
        let package_was_released_result =
            release_package_git_only(input, repo, git_client, &release_info)
                .await
                .context("failed to release package (git-only)")?;

        if package_was_released_result {
            package_was_released = true;
        }
    }

    let package_release = package_was_released.then_some(PackageRelease {
        package_name: package.name.to_string(),
        version: package.version.clone(),
        tag: git_tag,
        prs,
    });
    Ok(package_release)
}

/// Check if `package` is published in the primary index.
/// If the check fails, check the fallback index if it exists.
///
/// Returns whether the package is published and the index used for the check.
async fn is_package_published(
    input: &ReleaseRequest,
    package: &Package,
    mut primary_index: CargoIndex,
    fallback_index: Option<CargoIndex>,
    token: &Option<SecretString>,
) -> anyhow::Result<(bool, CargoIndex)> {
    let is_published_in_primary =
        is_published(&mut primary_index, package, input.publish_timeout, token).await;

    // If a fallback index is defined.
    if let Some(mut fallback_index) = fallback_index {
        // And if the primary index returns an error, attempt to check the
        // fallback.
        if let Err(e) = &is_published_in_primary {
            warn!(
                "Error checking primary index for package {}: {e:?}. Trying fallback index.",
                package.name
            );
            let is_published_in_fallback =
                is_published(&mut fallback_index, package, input.publish_timeout, token).await;
            if let Ok(fallback_is_published) = is_published_in_fallback {
                return Ok((fallback_is_published, fallback_index));
            }
        };
    };
    Ok((is_published_in_primary?, primary_index))
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

/// Get the indexes where the package should be published.
/// If `registry` is specified, it takes precedence over the `publish` field
/// of the package manifest.
fn registry_indexes(
    package: &Package,
    registry: Option<String>,
    registry_protocol: RegistryProtocol,
    hash_kind: &crates_index::HashKind,
) -> anyhow::Result<Vec<CargoRegistry>> {
    let registries = registry
        .map(|r| vec![r])
        .unwrap_or_else(|| package.publish.clone().unwrap_or_default());
    let registry_urls = if registries.is_empty() {
        let url = cargo_utils::registry_url(package.manifest_path.as_ref(), None)
            .context("failed to retrieve registry url")?;
        vec![(None, url)]
    } else {
        registries
            .into_iter()
            .map(|r| {
                cargo_utils::registry_url(package.manifest_path.as_ref(), Some(&r))
                    .context("failed to retrieve registry url")
                    .map(|url| (Some(r), url))
            })
            .collect::<anyhow::Result<Vec<(Option<String>, Url)>>>()?
    };

    registry_urls
        .into_iter()
        .map(|(registry, u)| get_cargo_registry(hash_kind, registry, &u, registry_protocol))
        .collect::<anyhow::Result<Vec<CargoRegistry>>>()
}

const CRATES_IO_GIT_INDEX_URL: &str = "https://github.com/rust-lang/crates.io-index";
const CRATES_IO_SPARSE_INDEX_URL: &str = "sparse+https://index.crates.io/";

fn is_crates_io_git_index(url: &str) -> bool {
    url.trim_end_matches('/') == CRATES_IO_GIT_INDEX_URL
}

fn get_cargo_registry(
    hash_kind: &crates_index::HashKind,
    registry: Option<String>,
    u: &Url,
    registry_protocol: RegistryProtocol,
) -> anyhow::Result<CargoRegistry> {
    let fallback_hash = try_get_fallback_hash_kind(hash_kind);
    let url_str = u.as_str();
    let (index_url, use_sparse) = if url_str.starts_with("sparse+") {
        (url_str.to_string(), true)
    } else {
        match registry_protocol {
            RegistryProtocol::Auto if is_crates_io_git_index(url_str) => {
                (CRATES_IO_SPARSE_INDEX_URL.to_string(), true)
            }
            RegistryProtocol::Git | RegistryProtocol::Auto => (format!("registry+{u}"), false),
            RegistryProtocol::Sparse => {
                if is_crates_io_git_index(url_str) {
                    (CRATES_IO_SPARSE_INDEX_URL.to_string(), true)
                } else {
                    let registry_hint = registry.as_deref().unwrap_or("unknown");
                    anyhow::bail!(
                        "registry '{registry_hint}' index does not support sparse protocol. Either set the index to a sparse+ URL in your Cargo config, or set publish_registry_protocol = \"git\" in release-plz config"
                    );
                }
            }
        }
    };

    let (maybe_primary_index, maybe_fallback_index) = if use_sparse {
        let maybe_primary =
            SparseIndex::from_url_with_hash_kind(&index_url, hash_kind).map(CargoIndex::Sparse);
        let maybe_fallback = fallback_hash.map(|hash_kind| {
            SparseIndex::from_url_with_hash_kind(&index_url, &hash_kind).map(CargoIndex::Sparse)
        });

        (maybe_primary, maybe_fallback)
    } else {
        let maybe_primary =
            GitIndex::from_url_with_hash_kind(&index_url, hash_kind).map(CargoIndex::Git);
        let maybe_fallback = fallback_hash.map(|hash_kind| {
            GitIndex::from_url_with_hash_kind(&index_url, &hash_kind).map(CargoIndex::Git)
        });

        (maybe_primary, maybe_fallback)
    };

    let primary_index = maybe_primary_index.context("failed to get cargo registry")?;

    let fallback_index = match maybe_fallback_index {
        // In cases where the primary index succeeds, the lookup should
        // continue regardless of the state of the fallback index.
        None | Some(Err(_)) => None,
        Some(Ok(fallback_index)) => Some(fallback_index),
    };

    Ok(CargoRegistry {
        name: registry,
        index: primary_index,
        fallback_index,
    })
}

struct ReleaseInfo<'a> {
    package: &'a Package,
    git_tag: &'a str,
    release_name: &'a str,
    changelog: &'a str,
    prs: &'a [Pr],
}

/// Return `true` if package was published, `false` otherwise.
#[allow(clippy::too_many_arguments)]
async fn release_package(
    index: &mut CargoIndex,
    input: &ReleaseRequest,
    repo: &Repo,
    git_client: &GitClient,
    release_info: &ReleaseInfo<'_>,
    token: &Option<SecretString>,
    is_crates_io: bool,
    trusted_publishing_client: &mut Option<trusted_publishing::TrustedPublisher>,
) -> anyhow::Result<bool> {
    let workspace_root = &input.metadata.workspace_root;

    let should_publish = input.is_publish_enabled(&release_info.package.name);
    let should_create_git_tag = input.is_git_tag_enabled(&release_info.package.name);
    let should_create_git_release = input.is_git_release_enabled(&release_info.package.name);

    let mut publish_token: Option<SecretString> = token.clone();
    let should_use_trusted_publishing = {
        let is_github_actions = std::env::var("GITHUB_ACTIONS").is_ok();
        publish_token.is_none()
            && input.token.is_none()
            && is_crates_io
            && should_publish
            && !input.dry_run
            && is_github_actions
    };
    if should_use_trusted_publishing {
        if let Some(tp) = trusted_publishing_client.as_ref() {
            publish_token = Some(tp.token().clone());
        } else {
            match trusted_publishing::TrustedPublisher::crates_io().await {
                Ok(tp) => {
                    publish_token = Some(tp.token().clone());
                    *trusted_publishing_client = Some(tp);
                }
                Err(e) => {
                    warn!("Failed to use trusted publishing: {e:#}. Proceeding without it.");
                }
            }
        }
    }

    if should_publish {
        // Run `cargo publish`. Note that `--dry-run` is added if `input.dry_run` is true.
        let output = run_cargo_publish(release_info.package, input, workspace_root, &publish_token)
            .context("failed to run cargo publish")?;
        if !output.status.success()
            || !output.stderr.contains("Uploading")
            || output.stderr.contains("error:")
        {
            if output.stderr.contains(&format!(
                "crate version `{}` is already uploaded",
                &release_info.package.version,
            )) {
                // The crate was published while `cargo publish` was running.
                // Note that the crate wasn't published yet when `cargo publish` started,
                // otherwise `cargo` would have returned the error "crate {package}@{version} already exists"
                info!(
                    "skipping publish of {} {}: already published",
                    release_info.package.name, release_info.package.version
                );
                return Ok(false);
            } else {
                anyhow::bail!(
                    "failed to publish {}: {}",
                    release_info.package.name,
                    output.stderr
                );
            }
        }
    }

    if input.dry_run {
        log_dry_run_info(
            release_info,
            should_publish,
            should_create_git_tag,
            should_create_git_release,
        );
        Ok(false)
    } else {
        if should_publish {
            wait_until_published(index, release_info.package, input.publish_timeout, token).await?;
        }

        create_git_tag_and_release(
            input,
            repo,
            git_client,
            release_info,
            should_create_git_tag,
            should_create_git_release,
        )
        .await?;

        info!(
            "published {} {}",
            release_info.package.name, release_info.package.version
        );
        Ok(true)
    }
}

/// Release a package without publishing to any registry (git-only mode).
/// This only creates git tags and git releases.
///
/// Return `true` if package was released, `false` otherwise.
async fn release_package_git_only(
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
            false, // should_publish is always false in git-only mode
            should_create_git_tag,
            should_create_git_release,
        );
        Ok(false)
    } else {
        create_git_tag_and_release(
            input,
            repo,
            git_client,
            release_info,
            should_create_git_tag,
            should_create_git_release,
        )
        .await?;

        info!(
            "released {} {} (git-only)",
            release_info.package.name, release_info.package.version
        );
        Ok(true)
    }
}

/// Create git tag and/or git release for a package.
async fn create_git_tag_and_release(
    input: &ReleaseRequest,
    repo: &Repo,
    git_client: &GitClient,
    release_info: &ReleaseInfo<'_>,
    should_create_git_tag: bool,
    should_create_git_release: bool,
) -> anyhow::Result<()> {
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

    if should_create_git_release {
        let contributors = get_contributors(release_info, git_client).await;

        // TODO fill the rest
        let remote = Remote {
            owner: String::new(),
            repo: String::new(),
            link: String::new(),
            contributors,
        };
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

    Ok(())
}

/// Traces the steps that would have been taken had release been run without dry-run.
fn log_dry_run_info(
    release_info: &ReleaseInfo,
    should_publish: bool,
    should_create_git_tag: bool,
    should_create_git_release: bool,
) {
    let prefix = format!(
        "{} {}:",
        release_info.package.name, release_info.package.version
    );

    let mut items_to_skip = vec![];

    if should_publish {
        items_to_skip.push("cargo registry upload".to_string());
    }

    if should_create_git_tag {
        items_to_skip.push(format!("creation of tag '{}'", release_info.git_tag));
    }

    if should_create_git_release {
        items_to_skip.push("creation of git release".to_string());
    }

    if items_to_skip.is_empty() {
        info!("{prefix} no release method enabled");
    } else {
        info!("{prefix} due to dry, skipping the following: {items_to_skip:?}",);
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

/// Return `Err` if the `CARGO_REGISTRY_TOKEN` environment variable is set to an empty string in CI.
/// Reason:
/// - If the token is set to an empty string, probably the user forgot to set the
///   secret in GitHub actions.
///   It is important to only check this before running a release because
///   for bots like dependabot, secrets are not visible. So, there are PRs that don't
///   need a release that don't have the token set.
/// - If the token is unset, the user might want to log in to the registry
///   with `cargo login`. Don't throw an error in this case.
fn verify_ci_cargo_registry_token() -> anyhow::Result<()> {
    let is_token_empty = std::env::var("CARGO_REGISTRY_TOKEN").map(|t| t.is_empty()) == Ok(true);
    let is_environment_github_actions = std::env::var("GITHUB_ACTIONS").is_ok();
    anyhow::ensure!(
        !(is_environment_github_actions && is_token_empty),
        "CARGO_REGISTRY_TOKEN environment variable is set to empty string. Please set your token in GitHub actions secrets. Docs: https://release-plz.dev/docs/github/quickstart#2-set-the-cargo_registry_token-secret"
    );
    Ok(())
}

fn run_cargo_publish(
    package: &Package,
    input: &ReleaseRequest,
    workspace_root: &Utf8Path,
    token: &Option<SecretString>,
) -> anyhow::Result<CmdOutput> {
    let mut args = vec!["publish"];
    args.push("--color");
    args.push("always");
    args.push("--manifest-path");
    args.push(package.manifest_path.as_ref());
    // We specify the package name to allow publishing root packages.
    // See https://github.com/release-plz/release-plz/issues/1545
    args.push("--package");
    args.push(&package.name);
    if let Some(registry) = &input.registry {
        args.push("--registry");
        args.push(registry);
    }
    if let Some(token) = token.as_ref().or(input.token.as_ref()) {
        args.push("--token");
        args.push(token.expose_secret());
    } else {
        verify_ci_cargo_registry_token()?;
    }
    if input.dry_run {
        args.push("--dry-run");
    }
    if input.allow_dirty(&package.name) {
        args.push("--allow-dirty");
    }
    if input.no_verify(&package.name) {
        args.push("--no-verify");
    }
    let features = input.features(&package.name).join(",");
    if !features.is_empty() {
        args.push("--features");
        args.push(&features);
    }
    if input.all_features(&package.name) {
        args.push("--all-features");
    }
    run_cargo(workspace_root, &args)
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
    use std::env;
    use std::ffi::OsStr;
    use std::sync::{LazyLock, Mutex};

    use fake_package::metadata::fake_metadata;

    use super::*;

    // Trick to avoid the tests to run concurrently.
    // It's used to not affect environment variables used in other tests
    // since tests run concurrently by default and share the same environment context.
    static NO_PARALLEL: LazyLock<Mutex<()>> = LazyLock::new(Mutex::default);

    fn with_env_var<K, V, F>(key: K, value: V, f: F)
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
        F: FnOnce(),
    {
        // Prevents concurrent runs where environment changes are made.
        // Caller assumes all environment changes are reset to their state
        // prior to calling this function when this guard is dropped.
        let _guard = NO_PARALLEL.lock().unwrap();

        // Store the previous value of the var, if defined.
        let previous_val = env::var(key.as_ref()).ok();

        unsafe { env::set_var(key.as_ref(), value.as_ref()) };
        (f)();

        // Reset or clear the var after the test.
        if let Some(previous_val) = previous_val {
            unsafe { env::set_var(key.as_ref(), previous_val) };
        } else {
            unsafe { env::remove_var(key.as_ref()) };
        }
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

    #[test]
    fn release_request_registry_token_env_works() {
        let registry_name = "my_registry";
        let token = "t0p$eCrEt";
        let token_env_var = format!("CARGO_REGISTRIES_{}_TOKEN", registry_name.to_uppercase());

        with_env_var(&token_env_var, token, || {
            let request = ReleaseRequest::new(fake_metadata()).with_registry(registry_name);
            let registry_token = request.find_registry_token(Some(registry_name)).unwrap();

            assert!(registry_token.is_some());
            assert_eq!(token, registry_token.unwrap().expose_secret());
        });
    }

    #[test]
    fn should_reference_env_var_provided_index() {
        use cargo_utils::registry_url;

        let registry_name = "my_registry";
        let mock_index = "https://example.com/git/index";
        let mock_index_url = Url::parse(mock_index).unwrap();

        let index_env_var = format!("CARGO_REGISTRIES_{}_INDEX", registry_name.to_uppercase());

        let fake_metadata = fake_metadata();
        let fake_manifest_path = fake_metadata.workspace_root.as_ref();

        with_env_var(&index_env_var, mock_index, || {
            let maybe_registry_index =
                registry_url(fake_manifest_path, Some(registry_name)).unwrap();

            // assert the registry index is properly overriden
            assert_eq!(maybe_registry_index, mock_index_url);
        });

        let non_overriden_maybe_registry_index =
            registry_url(fake_manifest_path, Some(registry_name)).ok();

        // assert the index is inherited from the workspace after the env var
        // is cleared.
        assert_eq!(non_overriden_maybe_registry_index, None);
    }

    #[test]
    fn check_publish_fields_works() {
        // fake_metadata() has `publish = false` in the Cargo.toml
        let mut request = ReleaseRequest::new(fake_metadata());
        request = request.with_package_config(
            "fake_package".to_string(),
            ReleaseConfig {
                publish: PublishConfig { enabled: true },
                ..Default::default()
            },
        );

        assert!(request.check_publish_fields().is_err());
    }
}
