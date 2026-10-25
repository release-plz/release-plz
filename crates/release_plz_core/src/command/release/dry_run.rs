use anyhow::Context as _;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use tracing::{info, warn};

use super::{PublishOptions, ReleaseRequest, run_cargo_publish};
use crate::cargo::{CmdOutput, run_cargo};

/// Possible cause of a failed check when packages are checked separately.
const SEPARATE_CHECKS_HINT: &str = "\nIf a package depends on an unpublished workspace package, \
    Cargo can only resolve the dependency if both packages are checked together. \
    Release-plz checks packages separately if their registry, publish_allow_dirty, \
    publish_no_verify or publish_all_features differ, if they set publish_features, \
    or if Cargo is older than 1.90.";

#[derive(Default)]
pub(super) struct PublishBatches {
    batches: Vec<PublishBatch>,
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
    pub(super) fn add(
        &mut self,
        package: &Package,
        registry: Option<String>,
        input: &ReleaseRequest,
    ) {
        let options = PublishOptions::new(input, &package.name);
        // Check packages with features individually, because selecting multiple packages
        // changes the meaning of `--features`: Cargo only requires one of the selected
        // packages to define each feature, and with resolver 1 it enables them only for
        // the package of the manifest.
        let can_combine = options.features.is_empty();
        if can_combine
            && let Some(batch) = self
                .batches
                .iter_mut()
                .find(|batch| batch.registry == registry && batch.options == options)
        {
            batch.packages.push(package.name.to_string());
        } else {
            self.batches.push(PublishBatch {
                registry,
                options,
                manifest_path: package.manifest_path.clone(),
                packages: vec![package.name.to_string()],
            });
        }
    }

    /// Run `cargo publish --dry-run` for every batch.
    pub(super) fn check(&self, input: &ReleaseRequest) -> anyhow::Result<()> {
        let has_combined_packages = self.batches.iter().any(|batch| batch.packages.len() > 1);
        // Only query the Cargo version when it matters, so that it can't break other dry runs.
        let check_individually = has_combined_packages
            && !can_publish_multiple_packages(&input.metadata.workspace_root)?;
        if check_individually {
            warn!(
                "checking each package individually, because checking packages together \
                 requires Cargo 1.90 or newer. Packages that depend on unpublished workspace \
                 packages fail the check."
            );
        }
        for batch in &self.batches {
            if check_individually {
                self.check_individually(input, batch)?;
            } else {
                self.check_together(input, batch)?;
            }
        }
        Ok(())
    }

    fn check_together(&self, input: &ReleaseRequest, batch: &PublishBatch) -> anyhow::Result<()> {
        let output = batch.dry_run(input, &batch.packages)?;
        // Cargo rejects selected packages that form a cycle, e.g. a proc-macro crate that
        // dev-depends on the crate depending on it. The real release publishes them one at
        // a time, so Cargo resolves the dev-dependency to the version in the registry.
        if batch.packages.len() > 1
            && !output.status.success()
            && output
                .stderr
                .contains("circular dependency detected while publishing")
        {
            info!(
                "checking {} individually, because Cargo can't check packages with a \
                 dependency cycle together",
                batch.packages.join(", ")
            );
            return self.check_individually(input, batch);
        }
        self.ensure_success(&output, &batch.packages)
    }

    fn check_individually(
        &self,
        input: &ReleaseRequest,
        batch: &PublishBatch,
    ) -> anyhow::Result<()> {
        for package in &batch.packages {
            let packages = std::slice::from_ref(package);
            let output = batch.dry_run(input, packages)?;
            self.ensure_success(&output, packages)?;
        }
        Ok(())
    }

    /// Return an error with the output of Cargo if the check of `packages` failed.
    fn ensure_success(&self, output: &CmdOutput, packages: &[String]) -> anyhow::Result<()> {
        if !output.status.success() {
            // Only hint if some unpublished packages weren't part of this check.
            let package_count: usize = self.batches.iter().map(|b| b.packages.len()).sum();
            let hint = if packages.len() < package_count {
                SEPARATE_CHECKS_HINT
            } else {
                ""
            };
            anyhow::bail!(
                "failed to dry-run publishing {}: {}{hint}",
                packages.join(", "),
                output.stderr
            );
        }
        Ok(())
    }
}

impl PublishBatch {
    /// Run `cargo publish --dry-run` for `packages` of this batch.
    fn dry_run(&self, input: &ReleaseRequest, packages: &[String]) -> anyhow::Result<CmdOutput> {
        let registry = self.registry.as_deref();
        let token = input.find_registry_token(registry)?;
        run_cargo_publish(
            input,
            &self.manifest_path,
            packages,
            &self.options,
            registry,
            token.as_ref(),
        )
        .context("failed to run cargo publish --dry-run")
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
