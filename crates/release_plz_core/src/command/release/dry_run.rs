use anyhow::Context as _;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use tracing::warn;

use super::{PublishOptions, ReleaseRequest, run_cargo_publish};
use crate::cargo::run_cargo;

/// Possible cause of a failed check when packages are checked separately.
const SEPARATE_CHECKS_HINT: &str = "\nIf a package depends on an unpublished workspace package, \
    Cargo can only resolve the dependency if both packages are checked together. \
    Release-plz checks packages separately if their registry, publish_allow_dirty, \
    publish_no_verify or publish_all_features differ, if they set publish_features, \
    or if Cargo is older than 1.90.";

pub(super) struct PublishBatches {
    batches: Vec<PublishBatch>,
    /// Whether Cargo can check multiple packages with a single `cargo publish`.
    can_combine_packages: bool,
}

struct PublishBatch {
    /// `--registry` of `cargo publish`, as in the real release.
    /// [`None`] lets Cargo select the registry.
    /// Don't resolve [`None`] to the registry Cargo selects: Cargo behaves differently
    /// without `--registry`, e.g. it refuses to publish if crates.io is source-replaced.
    registry: Option<String>,
    /// Cargo applies these options to every selected package. Don't combine
    /// packages when doing so would change their configured verification behavior.
    options: PublishOptions,
    /// Manifest of the first package, as in the real release.
    /// Cargo finds the other packages in the same workspace.
    manifest_path: Utf8PathBuf,
    packages: Vec<String>,
}

impl PublishBatches {
    pub(super) fn new(input: &ReleaseRequest) -> anyhow::Result<Self> {
        Ok(Self {
            batches: vec![],
            can_combine_packages: can_publish_multiple_packages(&input.metadata.workspace_root)?,
        })
    }

    pub(super) fn add(
        &mut self,
        package: &Package,
        registry: Option<String>,
        input: &ReleaseRequest,
    ) {
        let options = PublishOptions::new(input, &package.name);
        let existing_batch = self
            .batches
            .iter_mut()
            .find(|batch| batch.registry == registry && batch.options == options);
        match existing_batch {
            // Selecting multiple packages changes the meaning of `--features`: Cargo
            // only requires one of the selected packages to define each feature, and
            // with resolver 1 it enables them only for the package of the manifest.
            Some(batch) if self.can_combine_packages && options.features.is_empty() => {
                batch.packages.push(package.name.to_string());
            }
            _ => self.batches.push(PublishBatch {
                registry,
                options,
                manifest_path: package.manifest_path.clone(),
                packages: vec![package.name.to_string()],
            }),
        }
    }

    /// Run `cargo publish --dry-run` for every batch.
    pub(super) fn check(&self, input: &ReleaseRequest) -> anyhow::Result<()> {
        if !self.can_combine_packages && self.batches.len() > 1 {
            warn!(
                "checking each package individually, because checking packages together \
                 requires Cargo 1.90 or newer. Packages that depend on unpublished workspace \
                 packages fail the check."
            );
        }
        for batch in &self.batches {
            let registry = batch.registry.as_deref();
            let token = input.find_registry_token(registry)?;
            let output = run_cargo_publish(
                input,
                &batch.manifest_path,
                &batch.packages,
                &batch.options,
                registry,
                token.as_ref(),
            )
            .context("failed to run cargo publish --dry-run")?;
            if !output.status.success() {
                let hint = if self.batches.len() > 1 {
                    SEPARATE_CHECKS_HINT
                } else {
                    ""
                };
                anyhow::bail!(
                    "failed to dry-run publishing {}: {}{hint}",
                    batch.packages.join(", "),
                    output.stderr
                );
            }
        }
        Ok(())
    }
}

/// Whether Cargo can publish multiple packages with a single `cargo publish`,
/// which is supported since Cargo 1.90.
fn can_publish_multiple_packages(workspace_root: &Utf8Path) -> anyhow::Result<bool> {
    let output = run_cargo(workspace_root, &["--version"])?;
    anyhow::ensure!(output.status.success(), "cannot determine Cargo version");
    let version = output
        .stdout
        .split_whitespace()
        .nth(1)
        .context("missing Cargo version")?;
    let version = Version::parse(version).context("cannot parse Cargo version")?;
    // Ignore pre-release identifiers: in semver, `1.90.0-beta.1` is lower than `1.90.0`,
    // but Cargo 1.90 betas and nightlies support it too.
    Ok((version.major, version.minor) >= (1, 90))
}
