use std::path::{Path, PathBuf};

use clap::builder::{NonEmptyStringValueParser, PathBufValueParser};
use k_releaser_core::PublishRequest;
use secrecy::SecretString;

use crate::config::Config;

use super::{OutputType, config_path::ConfigPath, manifest_command::ManifestCommand};

#[derive(clap::Parser, Debug)]
pub struct Publish {
    /// Path to the Cargo.toml of the project you want to publish.
    /// If not provided, k-releaser will use the Cargo.toml of the current directory.
    /// Both Cargo workspaces and single packages are supported.
    #[arg(long, value_parser = PathBufValueParser::new(), alias = "project-manifest")]
    manifest_path: Option<PathBuf>,

    /// Registry where you want to publish the packages.
    /// The registry name needs to be present in the Cargo config.
    /// If unspecified, the `publish` field of the package manifest is used.
    /// If the `publish` field is empty, crates.io is used.
    #[arg(long)]
    registry: Option<String>,

    /// Token used to publish to the cargo registry.
    /// Override the `CARGO_REGISTRY_TOKEN` environment variable, or the `CARGO_REGISTRIES_<NAME>_TOKEN`
    /// environment variable, used for registry specified in the `registry` input variable.
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    token: Option<String>,

    /// Perform all checks without uploading.
    #[arg(long)]
    pub dry_run: bool,

    /// Don't verify the contents by building them.
    /// When you pass this flag, `k-releaser` adds the `--no-verify` flag to `cargo publish`.
    #[arg(long)]
    pub no_verify: bool,

    /// Allow dirty working directories to be packaged.
    /// When you pass this flag, `k-releaser` adds the `--allow-dirty` flag to `cargo publish`.
    #[arg(long)]
    pub allow_dirty: bool,

    /// Print the order packages would be published in and exit.
    /// Does not actually publish anything.
    #[arg(long)]
    pub print_order: bool,

    /// Path to the k-releaser config file.
    #[command(flatten)]
    pub config: ConfigPath,

    /// Output format. If specified, prints the version and the tag of the
    /// published packages.
    #[arg(short, long, value_enum)]
    pub output: Option<OutputType>,
}

impl Publish {
    pub fn publish_request(
        self,
        config: &Config,
        metadata: cargo_metadata::Metadata,
    ) -> anyhow::Result<PublishRequest> {
        let mut req = PublishRequest::new(metadata).with_dry_run(self.dry_run);

        if let Some(registry) = self.registry {
            req = req.with_registry(registry);
        }
        if let Some(token) = self.token {
            req = req.with_token(SecretString::from(token));
        }

        req = req.with_publish_timeout(config.workspace.publish_timeout()?);

        req = config.fill_publish_config(self.allow_dirty, self.no_verify, req);

        req.check_publish_fields()?;

        Ok(req)
    }
}

impl ManifestCommand for Publish {
    fn optional_manifest(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }
}
