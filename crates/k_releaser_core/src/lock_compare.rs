use std::collections::HashMap;

use anyhow::Context;
use cargo_metadata::camino::Utf8Path;
use serde::Deserialize;
use tracing::debug;

/// Compare the dependencies present in the `Cargo.lock` of the registry package and the local one.
/// Check if the dependencies of the registry package were updated.
/// This method doesn't detect if the local Cargo.lock added new packages: just if
/// the version of the packages changed.
/// This is enough to understand if the package was updated.
pub fn are_lock_dependencies_updated(
    local_lock: &Utf8Path,
    registry_package: &Utf8Path,
) -> anyhow::Result<bool> {
    let registry_lock = &registry_package.join("Cargo.lock");
    if !local_lock.exists() || !registry_lock.exists() {
        return Ok(false);
    }
    are_dependencies_updated(local_lock, registry_lock)
}

fn are_dependencies_updated(
    local_lock: &Utf8Path,
    registry_lock: &Utf8Path,
) -> anyhow::Result<bool> {
    let local_lock: Lockfile = read_lockfile(local_lock)
        .with_context(|| format!("failed to load lockfile of local package {local_lock:?}"))?;
    let registry_lock = read_lockfile(registry_lock).with_context(|| {
        format!("failed to load lockfile of registry package {registry_lock:?}")
    })?;
    let local_lock_packages = PackagesByName::new(&local_lock.packages);
    Ok(are_dependencies_of_lockfiles_updated(
        &registry_lock,
        &local_lock_packages,
    ))
}

fn read_lockfile(path: &Utf8Path) -> anyhow::Result<Lockfile> {
    let content = fs_err::read_to_string(path).context("can't read lockfile")?;
    let lockfile =
        toml::from_str(&content).with_context(|| format!("invalid format of lockfile {path:?}"))?;
    Ok(lockfile)
}

fn are_dependencies_of_lockfiles_updated(
    registry_lock: &Lockfile,
    local_lock: &PackagesByName,
) -> bool {
    // We iterate over registry packages, because the Cargo.lock of the
    // local package can have more packages.
    // In particular, the local package contains the dependencies of the dev dependencies.
    // If we iterate over local packages, this function will return true if
    // the dev dependencies contain a version of a package different from the one
    // used in the normal dependencies.
    for registry_package in &registry_lock.packages {
        if let Some(local_packages) = local_lock.get(&registry_package.name) {
            let is_same_version = local_packages
                .iter()
                .any(|p| p.version == registry_package.version);
            if !is_same_version {
                debug!(
                    "Version of package {} changed to version {:?}",
                    registry_package.name, registry_package.version
                );
                return true;
            }
        }
    }
    false
}

#[derive(Deserialize, Debug)]
struct Lockfile {
    #[serde(rename = "package")]
    packages: Vec<Package>,
}

#[derive(Deserialize, Debug)]
struct Package {
    name: String,
    version: String,
}

/// Packages grouped by name, to search faster.
/// Cargo.lock can contain multiple packages with the same name but different versions.
struct PackagesByName<'a> {
    packages: HashMap<&'a str, Vec<&'a Package>>,
}

impl<'a> PackagesByName<'a> {
    fn new(packages: &'a [Package]) -> Self {
        let mut packages_by_name = HashMap::new();
        for package in packages {
            packages_by_name
                .entry(package.name.as_str())
                .or_insert_with(Vec::new)
                .push(package);
        }
        Self {
            packages: packages_by_name,
        }
    }

    /// Get the packages with the given name.
    fn get(&self, name: &str) -> Option<&[&Package]> {
        self.packages.get(name).map(|p| {
            // If the entry exists, it contains at least one package.
            assert!(!p.is_empty());
            p.as_slice()
        })
    }
}
