use std::collections::{HashMap, HashSet};

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
    are_dependencies_updated(local_lock, registry_lock, None)
}

/// Compare only dependencies reachable from a package in a historical workspace lockfile.
pub(crate) fn are_workspace_lock_dependencies_updated(
    local_lock: &Utf8Path,
    released_lock: &Utf8Path,
    package_name: &str,
) -> anyhow::Result<bool> {
    if !local_lock.exists() || !released_lock.exists() {
        return Ok(false);
    }
    are_dependencies_updated(local_lock, released_lock, Some(package_name))
}

fn are_dependencies_updated(
    local_lock: &Utf8Path,
    registry_lock: &Utf8Path,
    package_name: Option<&str>,
) -> anyhow::Result<bool> {
    let mut local_lock: Lockfile = read_lockfile(local_lock)
        .with_context(|| format!("failed to load lockfile of local package {local_lock:?}"))?;
    let mut registry_lock = read_lockfile(registry_lock).with_context(|| {
        format!("failed to load lockfile of registry package {registry_lock:?}")
    })?;
    if let Some(package_name) = package_name {
        local_lock.retain_package_dependencies(package_name);
        registry_lock.retain_package_dependencies(package_name);
    }
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

impl Lockfile {
    fn retain_package_dependencies(&mut self, package_name: &str) {
        // Cargo omits version/source from dependency IDs when the name is unambiguous.
        let mut pending: Vec<_> = self
            .packages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.name == package_name && p.source.is_none())
            .map(|(i, _)| i)
            .collect();
        let mut reachable = HashSet::new();
        while let Some(index) = pending.pop() {
            if !reachable.insert(index) {
                continue;
            }
            for dependency in &self.packages[index].dependencies {
                pending.extend(
                    self.packages
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.matches_dependency(dependency))
                        .map(|(i, _)| i),
                );
            }
        }
        let mut index = 0;
        self.packages.retain(|_| {
            let keep = reachable.contains(&index);
            index += 1;
            keep
        });
    }
}

#[derive(Deserialize, Debug)]
struct Package {
    name: String,
    version: String,
    source: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

impl Package {
    fn matches_dependency(&self, dependency: &str) -> bool {
        let mut parts = dependency.splitn(3, ' ');
        parts.next() == Some(self.name.as_str())
            && parts.next().is_none_or(|version| version == self.version)
            && parts.next().is_none_or(|source| {
                self.source.as_deref().is_some_and(|s| {
                    // Dependency IDs omit the precise revision of a Git source.
                    let s = if s.starts_with("git+") {
                        s.split_once('#').map_or(s, |(url, _)| url)
                    } else {
                        s
                    };
                    source == format!("({s})")
                })
            })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_lock_comparison_follows_source_qualified_git_dependencies() {
        let directory = tempfile::tempdir().unwrap();
        let directory = Utf8Path::from_path(directory.path()).unwrap();
        let released = directory.join("released.lock");
        let local = directory.join("local.lock");
        // Cargo adds sources to dependency IDs when name and version are ambiguous,
        // but only the package source records the precise Git revision.
        let lockfile = r#"
version = 4
[[package]]
name = "binary"
version = "0.1.0"
dependencies = ["shared 0.1.0 (git+https://example.com/one)", "shared 0.1.0 (git+https://example.com/two?branch=next)"]
[[package]]
name = "shared"
version = "0.1.0"
source = "git+https://example.com/one#1111111111111111111111111111111111111111"
dependencies = ["leaf"]
[[package]]
name = "shared"
version = "0.1.0"
source = "git+https://example.com/two?branch=next#2222222222222222222222222222222222222222"
[[package]]
name = "leaf"
version = "1.0.0"
source = "git+https://example.com/one#1111111111111111111111111111111111111111"
"#;
        fs_err::write(&released, lockfile).unwrap();
        fs_err::write(&local, lockfile).unwrap();
        assert!(!are_workspace_lock_dependencies_updated(&local, &released, "binary").unwrap());
        fs_err::write(&local, lockfile.replace("1.0.0", "1.0.1")).unwrap();
        assert!(are_workspace_lock_dependencies_updated(&local, &released, "binary").unwrap());
    }

    #[test]
    fn workspace_lock_comparison_ignores_unrelated_packages() {
        let directory = tempfile::tempdir().unwrap();
        let directory = Utf8Path::from_path(directory.path()).unwrap();
        let released = directory.join("released.lock");
        let local = directory.join("local.lock");
        let lockfile = r#"
version = 4
[[package]]
name = "binary"
version = "0.1.0"
dependencies = ["library"]
[[package]]
name = "library"
version = "0.1.0"
dependencies = ["shared 1.0.0 (registry+https://example.com/index)"]
[[package]]
name = "shared"
version = "1.0.0"
source = "registry+https://example.com/index"
[[package]]
name = "shared"
version = "2.0.0"
source = "registry+https://example.com/index"
[[package]]
name = "unrelated"
version = "0.1.0"
dependencies = ["shared 2.0.0"]
"#;
        fs_err::write(&released, lockfile).unwrap();
        fs_err::write(
            &local,
            lockfile.replace("2.0.0", "2.0.1").replace(
                "name = \"unrelated\"\nversion = \"0.1.0\"",
                "name = \"unrelated\"\nversion = \"0.1.1\"",
            ),
        )
        .unwrap();
        assert!(!are_workspace_lock_dependencies_updated(&local, &released, "binary").unwrap());
        assert!(are_workspace_lock_dependencies_updated(&local, &released, "unrelated").unwrap());

        fs_err::write(&local, lockfile.replace("1.0.0", "1.0.1")).unwrap();
        assert!(are_workspace_lock_dependencies_updated(&local, &released, "binary").unwrap());
        assert!(!are_workspace_lock_dependencies_updated(&local, &released, "unrelated").unwrap());
    }
}
