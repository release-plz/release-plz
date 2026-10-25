use anyhow::Context as _;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use tracing::warn;

use super::{PublishOptions, ReleaseRequest, run_cargo_publish};
use crate::cargo::{new_cargo_config, run_cargo};

pub(super) struct PublishBatches {
    batches: Vec<PublishBatch>,
    /// Registry that Cargo uses for packages that don't select one.
    default_registry: String,
    /// Whether Cargo can check multiple packages with a single `cargo publish`.
    can_combine_packages: bool,
}

struct PublishBatch {
    registry: String,
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
        let workspace_root = &input.metadata.workspace_root;
        let default_registry = new_cargo_config(Some(workspace_root.clone()))?
            .default_registry()?
            .unwrap_or_else(|| "crates-io".to_string());
        Ok(Self {
            batches: vec![],
            default_registry,
            can_combine_packages: can_publish_multiple_packages(workspace_root)?,
        })
    }

    pub(super) fn add(
        &mut self,
        package: &Package,
        registry: Option<String>,
        input: &ReleaseRequest,
    ) {
        // Resolve Cargo's default before grouping: an implicit registry and the
        // same registry named explicitly must share their unpublished packages.
        let registry = registry.unwrap_or_else(|| self.default_registry.clone());
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
            let registry = Some(batch.registry.as_str());
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
            // Cargo's exit status covers the whole batch. In particular, do not
            // treat an already-published error for one member as batch success.
            let hint = if self.batches.len() > 1 {
                "\nUnpublished workspace dependencies must be selected in the same dry-run batch. \
                 They must use the same registry, publish_allow_dirty, publish_no_verify, \
                 publish_features and publish_all_features settings."
            } else {
                ""
            };
            anyhow::ensure!(
                output.status.success(),
                "failed to dry-run publishing {}: {}{hint}",
                batch.packages.join(", "),
                output.stderr
            );
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

#[cfg(test)]
mod tests {
    use fake_package::{FakePackage, metadata::fake_metadata};

    use super::*;
    use crate::fs_utils::Utf8TempDir;

    #[test]
    fn implicit_and_explicit_registries_share_a_batch() {
        let directory = Utf8TempDir::new().unwrap();
        let config = directory.path().join(".cargo/config.toml");
        fs_err::create_dir(config.parent().unwrap()).unwrap();
        let mut metadata = fake_metadata();
        metadata.workspace_root = directory.path().to_owned();
        let input = ReleaseRequest::new(metadata);
        for registry in ["crates-io", "private-registry"] {
            fs_err::write(&config, format!("[registry]\ndefault = {registry:?}\n")).unwrap();
            let mut batches = PublishBatches::new(&input).unwrap();
            batches.add(&FakePackage::new("one").into(), None, &input);
            batches.add(
                &FakePackage::new("two").into(),
                Some(registry.into()),
                &input,
            );
            batches.add(
                &FakePackage::new("three").into(),
                Some("other".into()),
                &input,
            );
            assert_eq!(batches.batches.len(), 2);
            assert_eq!(batches.batches[0].packages.len(), 2);
            assert_eq!(batches.batches[0].registry, registry);
        }
    }
}
