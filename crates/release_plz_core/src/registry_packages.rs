use std::collections::BTreeMap;

use anyhow::Context;
use cargo_metadata::{Package, camino::Utf8Path};
use git_cmd::git_in_dir;
use itertools::Itertools;
use tempfile::{TempDir, tempdir};

use crate::{PackagePath, cargo_vcs_info, download, next_ver};

#[derive(Debug, Default)]
pub struct PackagesCollection {
    packages: BTreeMap<String, RegistryPackage>,
    /// Packages might be downloaded and stored in a temporary directory.
    /// The directory is stored here so that it is deleted on drop
    _temp_dir: Option<TempDir>,
}

#[derive(Debug)]
pub struct RegistryPackage {
    pub package: Package,
    /// The SHA1 hash of the commit when the package was published.
    sha1: Option<String>,
}

impl RegistryPackage {
    pub fn new(package: Package, sha1: Option<String>) -> Self {
        Self { package, sha1 }
    }

    pub fn published_at_sha1(&self) -> Option<&str> {
        self.sha1.as_deref()
    }
}

impl PackagesCollection {
    pub fn get_package(&self, package_name: &str) -> Option<&Package> {
        self.packages.get(package_name).map(|p| &p.package)
    }

    pub fn get_registry_package(&self, package_name: &str) -> Option<&RegistryPackage> {
        self.packages.get(package_name)
    }

    pub fn with_packages(mut self, packages: BTreeMap<String, RegistryPackage>) -> Self {
        self.packages = packages;
        self
    }
}

/// Retrieve the latest version of the packages.
///
/// - If `registry_manifest` is provided, the packages are read from the local file system.
///   This is useful when the packages are already downloaded.
/// - Otherwise, the packages are downloaded from the cargo registry.
///
/// - If `registry` is provided, the packages are downloaded from the specified registry.
/// - Otherwise, the packages are downloaded from crates.io.
pub fn get_registry_packages(
    registry_manifest: Option<&Utf8Path>,
    local_packages: &[&Package],
    registry: Option<&str>,
) -> anyhow::Result<PackagesCollection> {
    let (temp_dir, registry_packages) = match registry_manifest {
        Some(manifest) => (
            None,
            next_ver::publishable_packages_from_manifest(manifest)?
                .into_iter()
                .map(|p| RegistryPackage {
                    package: p,
                    sha1: None,
                })
                .collect(),
        ),
        None => {
            let temp_dir = tempdir().context("failed to get a temporary directory")?;
            let directory = temp_dir.as_ref().to_str().context("invalid tempdir path")?;

            let registry_packages =
                download_packages_from_registry(local_packages, registry, directory)?;

            // After downloading the package, we initialize a git repo in the package.
            // This is because if cargo doesn't find a git repo in the package, it doesn't
            // show hidden files in `cargo package --list` output.
            let registry_packages = initialize_registry_package(registry_packages)
                .context("failed to initialize repository package")?;
            (Some(temp_dir), registry_packages)
        }
    };
    let registry_packages: BTreeMap<String, RegistryPackage> = registry_packages
        .into_iter()
        .map(|c| {
            let package_name = c.package.name.to_string();
            (package_name, c)
        })
        .collect();
    Ok(PackagesCollection {
        _temp_dir: temp_dir,
        packages: registry_packages,
    })
}

fn download_packages_from_registry(
    local_packages: &[&Package],
    registry: Option<&str>,
    directory: &str,
) -> anyhow::Result<Vec<Package>> {
    let packages_grouped_by_registry = local_packages.iter().chunk_by(|p| {
        // If registry is not provided, fallback to the Cargo.toml `publish` field.
        registry.or_else(|| {
            p.publish
                .as_ref()
                // Use the first registry in the `publish` field.
                .and_then(|p| p.first())
                .map(|x| x.as_str())
        })
    });

    // Clone from the different registries in parallel
    std::thread::scope(|scope| {
        let mut registry_packages: Vec<Package> = vec![];
        let mut handles = Vec::new();
        for (registry, packages) in &packages_grouped_by_registry {
            let packages_names: Vec<&str> = packages.map(|p| p.name.as_str()).collect();
            let mut downloader = download::PackageDownloader::new(packages_names, directory);
            if let Some(registry) = registry {
                downloader = downloader.with_registry(registry.to_string());
            }
            let handle = scope.spawn(move || downloader.download());
            handles.push(handle);
        }

        for handle in handles {
            let downloaded_packages = handle
                .join()
                .expect("Panicked while downloading packages")
                .context("Failed to download packages")?;
            registry_packages.extend(downloaded_packages);
        }
        Ok(registry_packages)
    })
}

fn initialize_registry_package(packages: Vec<Package>) -> anyhow::Result<Vec<RegistryPackage>> {
    let mut registry_packages = vec![];
    for p in packages {
        let package_path = p.package_path().unwrap();
        let cargo_vcs_info_path = package_path.join(".cargo_vcs_info.json");
        // cargo_vcs_info is only present if `cargo publish` wasn't used with
        // the `--allow-dirty` flag inside a git repo.
        let sha1 = if cargo_vcs_info_path.exists() {
            let sha1 = cargo_vcs_info::read_sha1_from_cargo_vcs_info(&cargo_vcs_info_path);
            // Remove the file, otherwise `cargo publish --list` fails
            fs_err::remove_file(cargo_vcs_info_path)?;
            sha1
        } else {
            None
        };
        let git_repo = package_path.join(".git");
        let commit_init = || git_in_dir(package_path, &["commit", "-m", "init"]);
        if !git_repo.exists() {
            git_in_dir(package_path, &["init"])?;
            git_in_dir(package_path, &["add", "."])?;
            if let Err(e) = commit_init()
                && e.to_string().trim().starts_with("Author identity unknown")
            {
                // we can use any email and name here, as this repository is only used
                // to compare packages
                git_in_dir(package_path, &["config", "user.email", "test@registry"])?;
                git_in_dir(package_path, &["config", "user.name", "test"])?;
                commit_init()?;
            }
        }
        registry_packages.push(RegistryPackage { package: p, sha1 });
    }
    Ok(registry_packages)
}
