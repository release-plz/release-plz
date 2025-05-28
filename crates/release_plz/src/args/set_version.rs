use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use cargo_metadata::{camino::Utf8Path, semver::Version};
use clap::builder::PathBufValueParser;
use release_plz_core::set_version::{SetVersionRequest, SetVersionSpec, VersionChange};

use crate::config::Config;

use super::{config_command::ConfigCommand, manifest_command::ManifestCommand};

#[derive(clap::Parser, Debug)]
pub struct SetVersion {
    /// New version of the package you want to update. Format: `<package_name>@<version-req>`.
    pub versions: Vec<String>,
    /// Path to the Cargo.toml of the project you want to update.
    /// If not provided, release-plz will use the Cargo.toml of the current directory.
    /// Both Cargo workspaces and single packages are supported.
    #[arg(long, value_parser = PathBufValueParser::new())]
    manifest_path: Option<PathBuf>,
    /// Path to the release-plz config file.
    /// Default: `./release-plz.toml`.
    /// If no config file is found, the default configuration is used.
    #[arg(
        long,
        value_name = "PATH",
        value_parser = PathBufValueParser::new()
    )]
    config: Option<PathBuf>,
}

impl ConfigCommand for SetVersion {
    fn config_path(&self) -> Option<&Utf8Path> {
        self.config.as_deref().and_then(|p| Utf8Path::from_path(p))
    }
}

impl SetVersion {
    fn parse_versions(self) -> anyhow::Result<SetVersionSpec> {
        let is_single_package = self.versions.len() == 1 && !self.versions[0].contains('@');
        if is_single_package {
            let version = Version::parse(&self.versions[0])?;
            Ok(SetVersionSpec::Single(VersionChange::new(version)))
        } else {
            let version_changes = self.parse_workspace_versions()?;
            Ok(SetVersionSpec::Workspace(version_changes))
        }
    }

    fn parse_workspace_versions(self) -> anyhow::Result<BTreeMap<String, VersionChange>> {
        self
            .versions
            .iter()
            .map(|v| {
                let error = Err(anyhow::anyhow!("version {v} is invalid. Format need to be `<package_name>@<version>`. E.g. `release-plz set-version serde@1.2.3`"));
                let d: Vec<_> = v.split('@').collect();
                #[expect(clippy::get_first)]
                let Some(package) = d.get(0) else {return error;};
                let Some(version) = d.get(1) else {return error;};
                let version = Version::parse(version)?;
                Ok((package.to_string(), VersionChange::new( version )))
            })
            .collect()
    }

    /// Get [`SetVersionRequest`]
    pub fn set_version_request(self, config: &Config) -> anyhow::Result<SetVersionRequest> {
        let cargo_metadata = self.cargo_metadata()?;
        let version_changes = self.parse_versions()?;
        let mut request = SetVersionRequest::new(version_changes, cargo_metadata)?;
        config.fill_set_version_config(&mut request)?;
        Ok(request)
    }
}

impl ManifestCommand for SetVersion {
    fn optional_manifest(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }
}
