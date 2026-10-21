//! Download packages from cargo registry, similar to the `git clone` behavior.

use std::{path::Path, thread};

use anyhow::{Context, anyhow};
use cargo_metadata::{Package, camino::Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use tracing::{info, instrument};

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
    pub async fn download(&self) -> anyhow::Result<Vec<Package>> {
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
            .map(|package_name| Crate::new(package_name.clone(), None))
            .collect();
        let mut cloner_builder = Cloner::builder()
            .with_directory(&self.directory)
            .with_source(source);
        if let Some(cwd) = &self.cargo_cwd {
            cloner_builder = cloner_builder.with_cargo_cwd(cwd.clone());
        }
        let cloner = cloner_builder.build().context("can't build cloner")?;
        let result = cloner.clone(&crates).await;
        // Dropping Cargo's context waits for its HTTP worker to finish the requests still
        // in flight, like the queries after a failed one. Don't make the caller wait.
        // If the thread can't start, `spawn` drops the cloner here instead.
        drop(
            thread::Builder::new()
                .name("release-plz-download".to_string())
                .spawn(move || drop(cloner)),
        );
        let package_dirs = result.context("error while downloading packages")?;

        read_packages(&package_dirs)
    }
}

/// Read the packages in `directories`, preserving their order.
///
/// Reading a package runs `cargo metadata`, so spread the reads across threads
/// instead of waiting for each subprocess in turn.
fn read_packages(directories: &[Utf8PathBuf]) -> anyhow::Result<Vec<Package>> {
    let parallelism = thread::available_parallelism().map_or(1, usize::from);
    // `chunks` panics on a size of 0, which an empty list would produce.
    let chunk_size = directories.len().div_ceil(parallelism).max(1);
    thread::scope(|scope| {
        let workers: Vec<_> = directories
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(|| {
                    chunk
                        .iter()
                        .map(read_package)
                        .collect::<anyhow::Result<Vec<_>>>()
                })
            })
            .collect();
        let mut packages = Vec::with_capacity(directories.len());
        // Joining in order returns the error of the first package that can't be
        // read, like reading the packages sequentially.
        for worker in workers {
            packages.extend(worker.join().expect("package reading thread panicked")?);
        }
        Ok(packages)
    })
}

/// Read a package from file system
pub fn read_package(directory: impl AsRef<Path>) -> anyhow::Result<Package> {
    let manifest_path = directory.as_ref().join(CARGO_TOML);
    let manifest_path = cargo_metadata::camino::Utf8PathBuf::from_path_buf(manifest_path)
        .map_err(|path| anyhow!("non-utf8 manifest path: {path:?}"))?;
    let metadata = cargo_utils::get_manifest_metadata(&manifest_path)
        .context("failed to execute cargo_metadata")?;
    let package = metadata
        .packages
        .first()
        .ok_or_else(|| anyhow!("cannot retrieve package at {:?}", directory.as_ref()))?;
    Ok(package.clone())
}
