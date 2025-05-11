use std::path::{Path, PathBuf};

use clap::builder::PathBufValueParser;

use super::manifest_command::ManifestCommand;

#[derive(clap::Parser, Debug)]
pub struct Init {
    /// Path to the Cargo.toml of the project you want to update.
    /// If not provided, release-plz will use the Cargo.toml of the current directory.
    /// Both Cargo workspaces and single packages are supported.
    #[arg(long, value_parser = PathBufValueParser::new())]
    manifest_path: Option<PathBuf>,
    /// If set, don't check if the toml files contain `description` and `license` fields, which are mandatory for crates.io.
    #[arg(long)]
    pub no_toml_check: bool,
    /// Creates a `release-plz.toml` file with default configuration.
    /// See <https://release-plz.dev/docs/config> for more information on the configuration file.
    #[arg(long)]
    pub config: bool,
}

impl ManifestCommand for Init {
    fn optional_manifest(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }
}
