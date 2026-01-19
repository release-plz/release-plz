use std::{collections::BTreeMap, time::Duration};

use anyhow::Context;
use cargo_metadata::{Metadata, Package, camino::Utf8Path};
use crates_index::{GitIndex, SparseIndex};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use tracing::{info, instrument, trace, warn};
use url::Url;

use crate::{
    Project, Publishable as _,
    cargo::{CargoIndex, CargoRegistry, CmdOutput, is_published, run_cargo, wait_until_published},
    cargo_hash_kind::{get_hash_kind, try_get_fallback_hash_kind},
    command::trusted_publishing,
};

use super::release::PublishConfig;

#[derive(Debug)]
pub struct PublishRequest {
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
    /// Package-specific configurations.
    packages_config: PackagesConfig,
    /// publish timeout
    publish_timeout: Duration,
}

impl PublishRequest {
    pub fn new(metadata: Metadata) -> Self {
        let minutes_30 = Duration::from_secs(30 * 60);
        Self {
            metadata,
            registry: None,
            token: None,
            dry_run: false,
            packages_config: PackagesConfig::default(),
            publish_timeout: minutes_30,
        }
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

    pub fn with_default_package_config(mut self, config: PublishPackageConfig) -> Self {
        self.packages_config.set_default(config);
        self
    }

    pub fn with_publish_timeout(mut self, timeout: Duration) -> Self {
        self.publish_timeout = timeout;
        self
    }

    /// Set publish config for a specific package.
    pub fn with_package_config(
        mut self,
        package: impl Into<String>,
        config: PublishPackageConfig,
    ) -> Self {
        self.packages_config.set(package.into(), config);
        self
    }

    fn is_publish_enabled(&self, package: &str) -> bool {
        let config = self.get_package_config(package);
        config.publish.is_enabled()
    }

    pub fn get_package_config(&self, package: &str) -> PublishPackageConfig {
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

    /// Checks for inconsistency in the `publish` fields in the workspace metadata and k-releaser config.
    ///
    /// If there is no inconsistency, returns Ok(())
    ///
    /// # Errors
    ///
    /// Errors if any package has `publish = false` or `publish = []` in the Cargo.toml
    /// but has `publish = true` in the k-releaser configuration.
    pub fn check_publish_fields(&self) -> anyhow::Result<()> {
        let publish_fields = self.packages_config.publish_overrides_fields();

        for package in &self.metadata.packages {
            if !package.is_publishable()
                && let Some(should_publish) = publish_fields.get(package.name.as_str())
            {
                anyhow::ensure!(
                    !should_publish,
                    "Package `{}` has `publish = false` or `publish = []` in the Cargo.toml, but it has `publish = true` in the k-releaser configuration.",
                    package.name
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PackagesConfig {
    /// Config for packages that don't have a specific configuration.
    default: PublishPackageConfig,
    /// Configurations that override `default`.
    /// The key is the package name.
    overrides: BTreeMap<String, PublishPackageConfig>,
}

impl PackagesConfig {
    fn get(&self, package_name: &str) -> PublishPackageConfig {
        self.overrides
            .get(package_name)
            .cloned()
            .unwrap_or(self.default.clone())
    }

    fn set_default(&mut self, config: PublishPackageConfig) {
        self.default = config;
    }

    fn set(&mut self, package_name: String, config: PublishPackageConfig) {
        self.overrides.insert(package_name, config);
    }

    // Return the `publish` fields explicitly set in the
    // `[[package]]` section of the k-releaser config.
    // I.e. `publish` isn't inherited from the `[workspace]` section of the
    // k-releaser config.
    fn publish_overrides_fields(&self) -> BTreeMap<String, bool> {
        self.overrides
            .iter()
            .map(|(package_name, publish_config)| {
                (package_name.clone(), publish_config.publish.is_enabled())
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PublishPackageConfig {
    publish: PublishConfig,
    /// Don't verify the contents by building them.
    /// If true, `k-releaser` adds the `--no-verify` flag to `cargo publish`.
    no_verify: bool,
    /// Allow dirty working directories to be packaged.
    /// If true, `k-releaser` adds the `--allow-dirty` flag to `cargo publish`.
    allow_dirty: bool,
    /// Features to be enabled when packaging the crate.
    /// If non-empty, pass the `--features` flag to `cargo publish`.
    features: Vec<String>,
    /// Enable all features when packaging the crate.
    /// If true, pass the `--all-features` flag to `cargo publish`.
    all_features: bool,
}

impl PublishPackageConfig {
    pub fn with_publish(mut self, publish: PublishConfig) -> Self {
        self.publish = publish;
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
}

#[derive(Serialize, Default, Debug)]
pub struct PublishOutput {
    published: Vec<PackagePublish>,
}

#[derive(Serialize, Debug)]
pub struct PackagePublish {
    package_name: String,
    version: String,
    /// Git tag name (format: package-vX.Y.Z)
    tag: String,
}

#[derive(Serialize, Debug)]
pub struct PublishOrderOutput {
    publish_order: Vec<PackageOrderInfo>,
}

#[derive(Serialize, Debug)]
pub struct PackageOrderInfo {
    name: String,
    path: String,
}

impl PublishOrderOutput {
    pub fn display(&self) -> String {
        let mut output = String::from("Packages will be published in this order:\n");
        for (idx, pkg) in self.publish_order.iter().enumerate() {
            output.push_str(&format!("{}. {} ({})\n", idx + 1, pkg.name, pkg.path));
        }
        output.push_str(&format!("\nTotal: {} packages", self.publish_order.len()));
        output
    }
}

/// Print the order packages would be published in.
#[instrument(skip(input))]
pub fn print_publish_order(input: &PublishRequest) -> anyhow::Result<PublishOrderOutput> {
    let overrides = input.packages_config.overridden_packages();
    let project = Project::new_for_publish(
        &cargo_utils::workspace_manifest(&input.metadata),
        None,
        &overrides,
        &input.metadata,
    )?;

    let packages = project.publishable_packages();

    if packages.is_empty() {
        anyhow::bail!("No publishable packages found in workspace");
    }

    let workspace_root = &input.metadata.workspace_root;
    let mut order_info = Vec::new();

    for package in packages {
        let relative_path = package
            .manifest_path
            .parent()
            .and_then(|p| p.strip_prefix(workspace_root).ok())
            .map(|p| p.to_string())
            .unwrap_or_else(|| ".".to_string());

        order_info.push(PackageOrderInfo {
            name: package.name.to_string(),
            path: relative_path,
        });
    }

    Ok(PublishOrderOutput {
        publish_order: order_info,
    })
}

/// Publish packages to cargo registry in dependency order.
#[instrument(skip(input))]
pub async fn publish(input: &PublishRequest) -> anyhow::Result<Option<PublishOutput>> {
    let overrides = input.packages_config.overridden_packages();
    // Project::new() already orders packages by dependency order
    let project = Project::new_for_publish(
        &cargo_utils::workspace_manifest(&input.metadata),
        None,
        &overrides,
        &input.metadata,
    )?;

    // Packages are already ordered by release order (dependencies first).
    let packages = project.publishable_packages();
    if packages.is_empty() {
        info!("nothing to publish");
        return Ok(None);
    }

    let mut package_publishes: Vec<PackagePublish> = vec![];
    let hash_kind = get_hash_kind()?;
    // The same trusted publishing token can be used for all packages.
    let mut trusted_publishing_client: Option<trusted_publishing::TrustedPublisher> = None;

    for package in packages {
        if let Some(pkg_publish) = publish_package_if_needed(
            input,
            &project,
            package,
            &hash_kind,
            &mut trusted_publishing_client,
        )
        .await?
        {
            package_publishes.push(pkg_publish);
        }
    }

    if let Some(tp) = trusted_publishing_client.as_ref()
        && let Err(e) = tp.revoke_token().await
    {
        warn!("Failed to revoke trusted publishing token: {e:?}");
    }

    let output = (!package_publishes.is_empty()).then_some(PublishOutput {
        published: package_publishes,
    });
    Ok(output)
}

async fn publish_package_if_needed(
    input: &PublishRequest,
    project: &Project,
    package: &Package,
    hash_kind: &crates_index::HashKind,
    trusted_publishing_client: &mut Option<trusted_publishing::TrustedPublisher>,
) -> anyhow::Result<Option<PackagePublish>> {
    let git_tag = project.git_tag(&package.version.to_string())?;

    let registry_indexes = registry_indexes(package, input.registry.clone(), hash_kind)
        .context("can't determine registry indexes")?;

    let mut package_was_published = false;

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
        let package_was_published_at_index = publish_package_to_registry(
            &mut index,
            input,
            package,
            &token,
            is_crates_io,
            trusted_publishing_client,
        )
        .await
        .context("failed to publish package")?;

        if package_was_published_at_index {
            package_was_published = true;
        }
    }

    let package_publish = package_was_published.then_some(PackagePublish {
        package_name: package.name.to_string(),
        version: package.version.to_string(),
        tag: git_tag,
    });
    Ok(package_publish)
}

/// Check if `package` is published in the primary index.
/// If the check fails, check the fallback index if it exists.
///
/// Returns whether the package is published and the index used for the check.
async fn is_package_published(
    input: &PublishRequest,
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

/// Return `true` if package was published, `false` otherwise.
async fn publish_package_to_registry(
    index: &mut CargoIndex,
    input: &PublishRequest,
    package: &Package,
    token: &Option<SecretString>,
    is_crates_io: bool,
    trusted_publishing_client: &mut Option<trusted_publishing::TrustedPublisher>,
) -> anyhow::Result<bool> {
    let workspace_root = &input.metadata.workspace_root;

    let should_publish = input.is_publish_enabled(&package.name);
    if !should_publish {
        trace!("{}: publishing disabled", package.name);
        return Ok(false);
    }

    let mut publish_token: Option<SecretString> = token.clone();
    let should_use_trusted_publishing = {
        let is_github_actions = std::env::var("GITHUB_ACTIONS").is_ok();
        publish_token.is_none()
            && input.token.is_none()
            && is_crates_io
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

    // Run `cargo publish`. Note that `--dry-run` is added if `input.dry_run` is true.
    let output = run_cargo_publish(package, input, workspace_root, &publish_token)
        .context("failed to run cargo publish")?;

    if !output.status.success()
        || !output.stderr.contains("Uploading")
        || output.stderr.contains("error:")
    {
        if output.stderr.contains(&format!(
            "crate version `{}` is already uploaded",
            &package.version,
        )) {
            // The crate was published while `cargo publish` was running.
            info!(
                "skipping publish of {} {}: already published",
                package.name, package.version
            );
            return Ok(false);
        } else {
            anyhow::bail!("failed to publish {}: {}", package.name, output.stderr);
        }
    }

    if input.dry_run {
        info!(
            "{} {}: dry run - skipping cargo registry upload",
            package.name, package.version
        );
        Ok(false)
    } else {
        wait_until_published(index, package, input.publish_timeout, token).await?;
        info!("published {} {}", package.name, package.version);
        Ok(true)
    }
}

/// Get the indexes where the package should be published.
/// If `registry` is specified, it takes precedence over the `publish` field
/// of the package manifest.
fn registry_indexes(
    package: &Package,
    registry: Option<String>,
    hash_kind: &crates_index::HashKind,
) -> anyhow::Result<Vec<CargoRegistry>> {
    let registries = registry
        .map(|r| vec![r])
        .unwrap_or_else(|| package.publish.clone().unwrap_or_default());
    let registry_urls = registries
        .into_iter()
        .map(|r| {
            cargo_utils::registry_url(package.manifest_path.as_ref(), Some(&r))
                .context("failed to retrieve registry url")
                .map(|url| (r, url))
        })
        .collect::<anyhow::Result<Vec<(String, Url)>>>()?;

    let mut registry_indexes = registry_urls
        .into_iter()
        .map(|(registry, u)| get_cargo_registry(hash_kind, registry, &u))
        .collect::<anyhow::Result<Vec<CargoRegistry>>>()?;
    if registry_indexes.is_empty() {
        registry_indexes.push(CargoRegistry {
            name: None,
            index: CargoIndex::Git(GitIndex::new_cargo_default()?),
            fallback_index: None,
        });
    }
    Ok(registry_indexes)
}

fn get_cargo_registry(
    hash_kind: &crates_index::HashKind,
    registry: String,
    u: &Url,
) -> anyhow::Result<CargoRegistry> {
    let fallback_hash = try_get_fallback_hash_kind(hash_kind);

    let (maybe_primary_index, maybe_fallback_index) = if u.to_string().starts_with("sparse+") {
        let index_url = u.as_str();
        let maybe_primary =
            SparseIndex::from_url_with_hash_kind(index_url, hash_kind).map(CargoIndex::Sparse);
        let maybe_fallback = fallback_hash.map(|hash_kind| {
            SparseIndex::from_url_with_hash_kind(index_url, &hash_kind).map(CargoIndex::Sparse)
        });

        (maybe_primary, maybe_fallback)
    } else {
        let index_url = format!("registry+{u}");
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

    let registry = CargoRegistry {
        name: Some(registry),
        index: primary_index,
        fallback_index,
    };
    Ok(registry)
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
    input: &PublishRequest,
    workspace_root: &Utf8Path,
    token: &Option<SecretString>,
) -> anyhow::Result<CmdOutput> {
    let mut args = vec!["publish"];
    args.push("--color");
    args.push("always");
    args.push("--manifest-path");
    args.push(package.manifest_path.as_ref());
    // We specify the package name to allow publishing root packages.
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

impl PackagesConfig {
    pub fn overridden_packages(&self) -> std::collections::HashSet<&str> {
        self.overrides.keys().map(|s| s.as_str()).collect()
    }
}
