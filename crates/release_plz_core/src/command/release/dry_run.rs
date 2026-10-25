use anyhow::Context as _;
use cargo_metadata::{Package, camino::Utf8PathBuf, semver::Version};

use super::{PublishOptions, ReleaseRequest, run_cargo_publish};
use crate::cargo::{new_cargo_config, run_cargo};

pub(super) struct PublishBatches {
    batches: Vec<PublishBatch>,
    /// Registry that Cargo uses for packages that don't select one.
    default_registry: String,
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
        let default_registry = new_cargo_config(Some(input.metadata.workspace_root.clone()))?
            .default_registry()?
            .unwrap_or_else(|| "crates-io".to_string());
        Ok(Self {
            batches: vec![],
            default_registry,
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
        if let Some(batch) = self
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

    pub(super) fn publish(&self, input: &ReleaseRequest) -> anyhow::Result<()> {
        if self.batches.iter().any(|batch| batch.packages.len() > 1) {
            let output = run_cargo(&input.metadata.workspace_root, &["--version"])?;
            anyhow::ensure!(output.status.success(), "cannot determine Cargo version");
            let version = output
                .stdout
                .split_whitespace()
                .nth(1)
                .context("missing Cargo version")?;
            let version = Version::parse(version).context("cannot parse Cargo version")?;
            anyhow::ensure!(
                version >= Version::new(1, 90, 0),
                "workspace release dry-runs require Cargo 1.90 or newer (found {version})"
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
