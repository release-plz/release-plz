use std::collections::{HashMap, HashSet};

use anyhow::Context;
use cargo_metadata::{DependencyKind, Metadata, camino::Utf8Path, semver::Version};
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
    let Some((local_lock, registry_lock)) = read_lockfiles(local_lock, registry_lock)? else {
        return Ok(false);
    };
    Ok(are_dependencies_updated(&local_lock, &registry_lock))
}

/// Compare only dependencies reachable from a package in a historical workspace lockfile.
pub(crate) fn are_workspace_lock_dependencies_updated(
    local_metadata: &Metadata,
    released_metadata: &Metadata,
    package_name: &str,
) -> anyhow::Result<bool> {
    let Some((mut local_lock, mut released_lock)) = read_lockfiles(
        &local_metadata.workspace_root.join("Cargo.lock"),
        &released_metadata.workspace_root.join("Cargo.lock"),
    )?
    else {
        return Ok(false);
    };
    local_lock.retain_package_dependencies(package_name, &local_metadata.packages);
    released_lock.retain_package_dependencies(package_name, &released_metadata.packages);
    Ok(are_dependencies_updated(&local_lock, &released_lock))
}

fn read_lockfiles(
    local_lock: &Utf8Path,
    registry_lock: &Utf8Path,
) -> anyhow::Result<Option<(Lockfile, Lockfile)>> {
    if !local_lock.exists() || !registry_lock.exists() {
        return Ok(None);
    }
    let local_lock = read_lockfile(local_lock)
        .with_context(|| format!("failed to load lockfile of local package {local_lock:?}"))?;
    let registry_lock = read_lockfile(registry_lock).with_context(|| {
        format!("failed to load lockfile of registry package {registry_lock:?}")
    })?;
    Ok(Some((local_lock, registry_lock)))
}

fn are_dependencies_updated(local_lock: &Lockfile, registry_lock: &Lockfile) -> bool {
    let local_lock_packages = PackagesByName::new(&local_lock.packages);
    are_dependencies_of_lockfiles_updated(registry_lock, &local_lock_packages)
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
    fn retain_package_dependencies(
        &mut self,
        package_name: &str,
        workspace_packages: &[cargo_metadata::Package],
    ) {
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
            let package = &self.packages[index];
            let workspace_package = workspace_packages.iter().find(|p| {
                package.source.is_none() && p.name == package.name && p.version == package.version
            });
            for dependency in &package.dependencies {
                pending.extend(
                    self.packages
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| {
                            p.matches_dependency(dependency)
                                && (package.name == package_name
                                    || workspace_package.is_none_or(|workspace_package| {
                                        !p.is_dev_only_dependency(workspace_package)
                                    }))
                        })
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
    version: Version,
    source: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

impl Package {
    fn is_dev_only_dependency(&self, package: &cargo_metadata::Package) -> bool {
        // Workspace lockfiles include each member's dev dependencies, but those
        // are not built when that member is used as another package's dependency.
        // Renamed dependencies can use the same crate from different versions or
        // sources, so match both before deciding which dependency kinds apply.
        let matching = package.dependencies.iter().filter(|dependency| {
            dependency.name == self.name
                && (dependency.req == cargo_metadata::semver::VersionReq::STAR
                    || dependency.req.matches(&self.version))
        });
        let matches_source = |dependency: &cargo_metadata::Dependency| {
            self.source.as_deref().map(source_without_revision)
                == dependency
                    .source
                    .as_ref()
                    .map(|source| source_without_revision(&source.repr))
        };
        // A patch can replace the declared source. If no source matches, only
        // discard the dependency when every matching declaration is dev-only.
        let has_source_match = matching.clone().any(matches_source);
        let mut matching = matching.filter(|d| !has_source_match || matches_source(d));
        matching
            .clone()
            .any(|d| d.kind == DependencyKind::Development)
            && matching.all(|d| d.kind == DependencyKind::Development)
    }

    fn matches_dependency(&self, dependency: &str) -> bool {
        let mut parts = dependency.splitn(3, ' ');
        parts.next() == Some(self.name.as_str())
            && parts
                .next()
                .is_none_or(|version| version == self.version.to_string())
            && parts.next().is_none_or(|source| {
                self.source
                    .as_deref()
                    .is_some_and(|s| source == format!("({})", source_without_revision(s)))
            })
    }
}

fn source_without_revision(source: &str) -> &str {
    // Metadata declarations and lockfile dependency IDs omit the resolved Git
    // revision. Preserve query parameters identifying a branch, tag, or rev.
    if source.starts_with("git+") {
        source.split_once('#').map_or(source, |(url, _)| url)
    } else {
        source
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

    fn compare_workspace_locks(local: &Utf8Path, released: &Utf8Path, package: &str) -> bool {
        let (mut local, mut released) = read_lockfiles(local, released).unwrap().unwrap();
        local.retain_package_dependencies(package, &[]);
        released.retain_package_dependencies(package, &[]);
        are_dependencies_updated(&local, &released)
    }

    #[test]
    fn workspace_lock_comparison_ignores_transitive_dev_dependencies() {
        let local = crate::fs_utils::Utf8TempDir::new().unwrap();
        let released = crate::fs_utils::Utf8TempDir::new().unwrap();
        for directory in [local.path(), released.path()] {
            fs_err::write(
                directory.join("Cargo.toml"),
                "[workspace]\nmembers = [\"binary\", \"library\"]\nresolver = \"2\"\n\
                 [patch.crates-io]\npatched-test = { git = \"https://example.com/patched-test\" }\n",
            )
            .unwrap();
            for (name, dependencies) in [
                (
                    "binary",
                    r#"
[dependencies]
library = { path = "../library" }
[dev-dependencies]
root-dev = "1"
"#,
                ),
                (
                    "library",
                    r#"
[dependencies]
shared = "1"
both = "1"
[build-dependencies]
builder = "1"
[dev-dependencies]
test-only = "1"
patched-test = "1"
both = "1"
shared-test = { package = "shared", version = "2" }
"#,
                ),
            ] {
                let package = directory.join(name);
                fs_err::create_dir_all(package.join("src")).unwrap();
                fs_err::write(package.join("src/lib.rs"), "").unwrap();
                fs_err::write(
                    package.join("Cargo.toml"),
                    format!(
                        "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2021\"\n{dependencies}"
                    ),
                )
                .unwrap();
            }
        }
        // No dependency resolution or registry access is needed to read dependency kinds.
        let local_metadata =
            cargo_utils::get_manifest_metadata(&local.path().join("Cargo.toml")).unwrap();
        let released_metadata =
            cargo_utils::get_manifest_metadata(&released.path().join("Cargo.toml")).unwrap();
        let mut lockfile = r#"
version = 4
[[package]]
name = "binary"
version = "0.1.0"
dependencies = ["library", "root-dev"]
[[package]]
name = "library"
version = "0.1.0"
dependencies = ["shared 1.0.0", "shared 2.0.0", "both", "builder", "test-only", "patched-test"]
[[package]]
name = "patched-test"
version = "1.0.0"
source = "git+https://example.com/patched-test#0123456789abcdef"
"#
        .to_string();
        for (name, version) in [
            ("shared", "1.0.0"),
            ("shared", "2.0.0"),
            ("both", "1.0.0"),
            ("builder", "1.0.0"),
            ("test-only", "1.0.0"),
            ("root-dev", "1.0.0"),
        ] {
            lockfile.push_str(&format!(
                "\n[[package]]\nname = {name:?}\nversion = {version:?}\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
            ));
        }
        fs_err::write(released.path().join("Cargo.lock"), &lockfile).unwrap();

        for (name, version, should_update) in [
            ("test-only", "1.0.0", false),
            // A registry dev dependency can resolve to a different source via [patch].
            ("patched-test", "1.0.0", false),
            ("shared", "2.0.0", false),
            ("shared", "1.0.0", true),
            ("both", "1.0.0", true),
            ("builder", "1.0.0", true),
            ("root-dev", "1.0.0", true),
        ] {
            let mut updated_version = Version::parse(version).unwrap();
            updated_version.patch += 1;
            let changed = lockfile
                .replace(
                    &format!("name = {name:?}\nversion = {version:?}"),
                    &format!("name = {name:?}\nversion = \"{updated_version}\""),
                )
                .replace(
                    &format!("{name} {version}"),
                    &format!("{name} {updated_version}"),
                );
            fs_err::write(local.path().join("Cargo.lock"), changed).unwrap();
            assert_eq!(
                are_workspace_lock_dependencies_updated(
                    &local_metadata,
                    &released_metadata,
                    "binary"
                )
                .unwrap(),
                should_update,
                "updating {name} {version}"
            );
        }
    }

    #[test]
    fn workspace_lock_comparison_distinguishes_dependency_sources() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        fs_err::write(
            directory.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"binary\", \"library\", \"shared\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        for name in ["binary", "library", "shared"] {
            let package = directory.path().join(name);
            fs_err::create_dir_all(package.join("src")).unwrap();
            fs_err::write(package.join("src/lib.rs"), "").unwrap();
            fs_err::write(
                package.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nversion = \"1.0.0\"\nedition = \"2021\"\n"),
            )
            .unwrap();
        }
        fs_err::write(
            directory.path().join("binary/Cargo.toml"),
            "[package]\nname = \"binary\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\
             [dependencies]\nlibrary = { path = \"../library\" }\n",
        )
        .unwrap();
        let path = "path = \"../shared\"";
        let registry = "version = \"1\"";
        let git = "git = \"https://example.com/shared\"";
        let other_git = "git = \"https://example.com/other\"";
        let branch = "git = \"https://example.com/shared\", branch = \"next\"";
        for (normal, dev) in [
            (git, other_git),
            (git, branch),
            (registry, git),
            (git, registry),
            (path, git),
            (git, path),
            (path, registry),
            (registry, path),
        ] {
            fs_err::write(
                directory.path().join("library/Cargo.toml"),
                format!(
                    "[package]\nname = \"library\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\
                     [dependencies]\nshared = {{ {normal} }}\n\
                     [dev-dependencies]\nshared-test = {{ package = \"shared\", {dev} }}\n"
                ),
            )
            .unwrap();
            // Read real dependency declarations without resolving or fetching them.
            let metadata =
                cargo_utils::get_manifest_metadata(&directory.path().join("Cargo.toml")).unwrap();
            let library = metadata
                .packages
                .iter()
                .find(|p| p.name == "library")
                .unwrap();
            let mut dependency_ids = Vec::new();
            let mut lockfile = String::from(
                "version = 4\n[[package]]\nname = \"binary\"\nversion = \"1.0.0\"\ndependencies = [\"library\"]\n",
            );
            for dependency in &library.dependencies {
                let leaf = if dependency.kind == DependencyKind::Development {
                    "dev-leaf"
                } else {
                    "normal-leaf"
                };
                let source = dependency.source.as_ref().map(|s| s.repr.as_str());
                dependency_ids.push(source.map_or_else(
                    || "shared 1.0.0".to_string(),
                    |source| format!("shared 1.0.0 ({source})"),
                ));
                lockfile.push_str("[[package]]\nname = \"shared\"\nversion = \"1.0.0\"\n");
                if let Some(source) = source {
                    let precise = if source.starts_with("git+") {
                        "#0123456789abcdef"
                    } else {
                        ""
                    };
                    lockfile.push_str(&format!("source = \"{source}{precise}\"\n"));
                }
                lockfile.push_str(&format!(
                    "dependencies = [{leaf:?}]\n[[package]]\nname = {leaf:?}\nversion = \"1.0.0\"\n"
                ));
            }
            lockfile.push_str(&format!(
                "[[package]]\nname = \"library\"\nversion = \"1.0.0\"\ndependencies = {dependency_ids:?}\n"
            ));
            let mut released: Lockfile = toml::from_str(&lockfile).unwrap();
            released.retain_package_dependencies("binary", &metadata.packages);
            for (leaf, should_update) in [("dev-leaf", false), ("normal-leaf", true)] {
                let changed = lockfile.replace(
                    &format!("name = {leaf:?}\nversion = \"1.0.0\""),
                    &format!("name = {leaf:?}\nversion = \"1.0.1\""),
                );
                let mut local: Lockfile = toml::from_str(&changed).unwrap();
                local.retain_package_dependencies("binary", &metadata.packages);
                assert_eq!(
                    are_dependencies_updated(&local, &released),
                    should_update,
                    "updating {leaf} with normal {normal} and dev {dev}"
                );
            }
        }
    }

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
        assert!(!compare_workspace_locks(&local, &released, "binary"));
        fs_err::write(&local, lockfile.replace("1.0.0", "1.0.1")).unwrap();
        assert!(compare_workspace_locks(&local, &released, "binary"));
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
        assert!(!compare_workspace_locks(&local, &released, "binary"));
        assert!(compare_workspace_locks(&local, &released, "unrelated"));

        fs_err::write(&local, lockfile.replace("1.0.0", "1.0.1")).unwrap();
        assert!(compare_workspace_locks(&local, &released, "binary"));
        assert!(!compare_workspace_locks(&local, &released, "unrelated"));
    }
}
