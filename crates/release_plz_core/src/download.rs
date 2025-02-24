//! Download packages from cargo registry, similar to the `git clone` behavior.

use std::path::Path;

use anyhow::{Context, anyhow};
use cargo_metadata::{Package, camino::Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use tracing::{info, instrument, warn};

use crate::clone::{Cloner, ClonerSource, Crate};

#[derive(Debug)]
pub struct PackageDownloader {
    packages: Vec<String>,
    directory: String,
    registry: Option<String>,
    cargo_cwd: Option<Utf8PathBuf>,
}

impl PackageDownloader {
    pub fn new(
        packages: impl IntoIterator<Item = impl Into<String>>,
        directory: impl Into<String>,
    ) -> Self {
        Self {
            packages: packages.into_iter().map(Into::into).collect(),
            directory: directory.into(),
            registry: None,
            cargo_cwd: None,
        }
    }

    pub fn with_registry(self, registry: String) -> Self {
        Self {
            registry: Some(registry),
            ..self
        }
    }

    pub fn with_cargo_cwd(self, cargo_cwd: Utf8PathBuf) -> Self {
        Self {
            cargo_cwd: Some(cargo_cwd),
            ..self
        }
    }

    #[instrument]
    pub fn download(&self) -> anyhow::Result<Vec<Package>> {
        let source: ClonerSource = match &self.registry {
            Some(registry) => ClonerSource::registry(registry),
            None => ClonerSource::crates_io(),
        };
        info!(
            "downloading packages from cargo registry {}",
            source.cargo_source
        );
        let crates: Vec<Crate> = self
            .packages
            .iter()
            .map(|package_name| Crate::new(package_name.to_string(), None))
            .collect();
        let mut cloner_builder = Cloner::builder()
            .with_directory(&self.directory)
            .with_source(source);
        if let Some(cwd) = &self.cargo_cwd {
            cloner_builder = cloner_builder.with_cargo_cwd(cwd.clone());
        }
        let downloaded_packages = cloner_builder
            .build()
            .context("can't build cloner")?
            .clone(&crates)
            .context("error while downloading packages")?;

        downloaded_packages
            .iter()
            .map(|(_package, path)| read_package(path))
            .collect()
    }
}

/// Read a package from file system
pub fn read_package(directory: impl AsRef<Path>) -> anyhow::Result<Package> {
    let manifest_path = directory.as_ref().join(CARGO_TOML);
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .manifest_path(manifest_path)
        .exec()
        .context("failed to execute cargo_metadata")?;
    let package = metadata
        .packages
        .first()
        .ok_or_else(|| anyhow!("cannot retrieve package at {:?}", directory.as_ref()))?;
    Ok(package.clone())
}

#[cfg(test)]
mod tests {
    use fake::Fake;
    use tempfile::tempdir;

    use super::*;

    #[test]
    #[ignore = "requires network"]
    fn one_package_is_downloaded() {
        let package_name = "rand";
        let temp_dir = tempdir().unwrap();
        let directory = temp_dir.as_ref().to_str().expect("invalid tempdir path");
        let packages = PackageDownloader::new([package_name], directory)
            .download()
            .unwrap();
        let rand = &packages[0];
        assert_eq!(rand.name, package_name);
    }

    #[test]
    #[ignore = "requires network"]
    fn two_packages_are_downloaded() {
        let first_package = "rand";
        let second_package = "rust-gh-example";
        let temp_dir = tempdir().unwrap();
        let directory = temp_dir.as_ref().to_str().expect("invalid tempdir path");
        let packages = PackageDownloader::new([first_package, second_package], directory)
            .download()
            .unwrap();
        assert_eq!(&packages[0].name, first_package);
        assert_eq!(&packages[1].name, second_package);
    }

    #[test]
    #[ignore = "requires network"]
    fn downloading_non_existing_package_does_not_error() {
        // Generate random string 15 characters long.
        let package: String = 15.fake();
        let temp_dir = tempdir().unwrap();
        let directory = temp_dir.as_ref().to_str().expect("invalid tempdir path");
        PackageDownloader::new([&package], directory)
            .download()
            .unwrap();
    }
}
