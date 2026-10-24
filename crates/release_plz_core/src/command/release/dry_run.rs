use anyhow::Context as _;
use cargo_metadata::{Package, semver::Version};

use super::{ReleaseRequest, run_cargo_publish};
use crate::cargo::{new_cargo_config, run_cargo};

#[derive(Default)]
pub(super) struct PublishBatches(Vec<PublishBatch>);

struct PublishBatch {
    registry: String,
    options: PublishOptions,
    packages: Vec<Package>,
}

/// Cargo applies these options to every selected package. Do not combine
/// packages when doing so would change their configured verification behavior.
#[derive(PartialEq, Eq)]
struct PublishOptions {
    allow_dirty: bool,
    no_verify: bool,
    features: Vec<String>,
    all_features: bool,
}

impl PublishBatches {
    pub(super) fn add(
        &mut self,
        package: &Package,
        registry: Option<String>,
        input: &ReleaseRequest,
    ) -> anyhow::Result<()> {
        // Resolve Cargo's default before grouping: an implicit registry and the
        // same registry named explicitly must share their unpublished packages.
        let registry = match registry {
            Some(registry) => registry,
            None => new_cargo_config(Some(input.metadata.workspace_root.clone()))?
                .default_registry()?
                .unwrap_or_else(|| "crates-io".to_string()),
        };
        let options = PublishOptions {
            allow_dirty: input.allow_dirty(&package.name),
            no_verify: input.no_verify(&package.name),
            features: input.features(&package.name),
            all_features: input.all_features(&package.name),
        };
        if let Some(batch) = self
            .0
            .iter_mut()
            .find(|batch| batch.registry == registry && batch.options == options)
        {
            batch.packages.push(package.clone());
        } else {
            self.0.push(PublishBatch {
                registry,
                options,
                packages: vec![package.clone()],
            });
        }
        Ok(())
    }

    pub(super) fn publish(&self, input: &ReleaseRequest) -> anyhow::Result<()> {
        if self.0.iter().any(|batch| batch.packages.len() > 1) {
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
        for batch in &self.0 {
            let registry = Some(batch.registry.as_str());
            let token = input.find_registry_token(registry)?;
            let output = run_cargo_publish(
                &batch.packages,
                input,
                &input.metadata.workspace_root,
                &token,
                registry,
            )
            .context("failed to run cargo publish --dry-run")?;
            // Cargo's exit status covers the whole batch. In particular, do not
            // treat an already-published error for one member as batch success.
            let hint = if self.0.len() > 1 {
                "\nUnpublished workspace dependencies must be selected in the same dry-run batch. \
                 They must use the same registry, publish_allow_dirty, publish_no_verify, \
                 publish_features and publish_all_features settings."
            } else {
                ""
            };
            anyhow::ensure!(
                output.status.success(),
                "failed to dry-run publishing {}: {}{hint}",
                batch
                    .packages
                    .iter()
                    .map(|package| package.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
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
            let mut batches = PublishBatches::default();
            batches
                .add(&FakePackage::new("one").into(), None, &input)
                .unwrap();
            batches
                .add(
                    &FakePackage::new("two").into(),
                    Some(registry.into()),
                    &input,
                )
                .unwrap();
            batches
                .add(
                    &FakePackage::new("three").into(),
                    Some("other".into()),
                    &input,
                )
                .unwrap();
            assert_eq!(batches.0.len(), 2);
            assert_eq!(batches.0[0].packages.len(), 2);
            assert_eq!(batches.0[0].registry, registry);
        }
    }
}
