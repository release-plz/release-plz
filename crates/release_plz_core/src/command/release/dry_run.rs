use std::collections::HashSet;

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
    Learn more at https://release-plz.dev/docs/usage/release#dry-runs";

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
    /// Cargo applies these options to every selected package, so only packages
    /// with equal options share a batch.
    options: PublishOptions,
    /// `--manifest-path` of `cargo publish`: the manifest of the first package of the batch.
    /// The checks of the other packages can reuse it, because Cargo resolves `--package`
    /// across the workspace. Packages with `--features`, whose meaning depends on the
    /// manifest under resolver 1, never share a batch.
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
        let combine =
            has_combined_packages && can_publish_multiple_packages(&input.metadata.workspace_root)?;
        if has_combined_packages && !combine {
            warn!(
                "checking each package individually, because checking packages together \
                 requires Cargo 1.90 or newer. Packages that depend on unpublished workspace \
                 packages fail the check."
            );
        }
        // A package published to multiple registries is in multiple batches.
        let package_count = self
            .batches
            .iter()
            .flat_map(|batch| &batch.packages)
            .collect::<HashSet<_>>()
            .len();
        for batch in &self.batches {
            batch.check(input, combine, package_count)?;
        }
        Ok(())
    }
}

impl PublishBatch {
    /// Run `cargo publish --dry-run` for the packages of this batch: together if
    /// `combine` is true, otherwise one at a time.
    /// `package_count` is the number of unpublished packages of the release.
    fn check(
        &self,
        input: &ReleaseRequest,
        combine: bool,
        package_count: usize,
    ) -> anyhow::Result<()> {
        if combine && self.packages.len() > 1 {
            let output = self.dry_run(input, &self.packages)?;
            // Cargo rejects selected packages that form a cycle, e.g. a proc-macro crate that
            // dev-depends on the crate depending on it. The real release publishes them one at
            // a time, so Cargo resolves the dev-dependency to the version in the registry.
            if output.status.success() || !is_cycle_error(&output.stderr) {
                return ensure_success(&output, &self.packages, package_count);
            }
            info!(
                "checking {} individually, because Cargo can't check packages with a \
                 dependency cycle together",
                self.packages.join(", ")
            );
        }
        for package in &self.packages {
            let packages = std::slice::from_ref(package);
            ensure_success(&self.dry_run(input, packages)?, packages, package_count)?;
        }
        Ok(())
    }

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

/// Return an error with the output of Cargo if the check of `packages` failed.
/// `package_count` is the number of unpublished packages of the release.
fn ensure_success(
    output: &CmdOutput,
    packages: &[String],
    package_count: usize,
) -> anyhow::Result<()> {
    if !output.status.success() {
        // Only hint if some unpublished packages weren't part of this check.
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

/// Whether `cargo publish --dry-run` failed because the selected packages form a cycle.
fn is_cycle_error(stderr: &str) -> bool {
    // Reported by Cargo 1.96 and newer.
    stderr.contains("circular dependency detected while publishing")
        // Reported by Cargo 1.90 to 1.95. Dry runs don't wait for the registry: they consider
        // every checked package available, so they only "time out" if no remaining package
        // has all its dependencies checked, i.e. if the remaining packages form a cycle.
        || stderr.contains(
            "due to a timeout while waiting for published dependencies to be available",
        )
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
