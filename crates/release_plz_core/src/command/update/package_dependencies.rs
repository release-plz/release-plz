use cargo_metadata::{Package, camino::Utf8Path, semver::Version};
use cargo_utils::LocalManifest;
use toml_edit::TableLike;

use crate::PackagePath as _;

pub trait PackageDependencies {
    /// Returns the `updated_packages` which should be updated in the dependencies of the package.
    /// Git-only releases also propagate changes through dependencies without version requirements.
    fn dependencies_to_update<'a>(
        &self,
        updated_packages: &'a [(&Package, Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
        include_versionless: bool,
    ) -> anyhow::Result<Vec<&'a Package>>;
}

impl PackageDependencies for Package {
    fn dependencies_to_update<'a>(
        &self,
        updated_packages: &'a [(&Package, Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
        include_versionless: bool,
    ) -> anyhow::Result<Vec<&'a Package>> {
        // Look into the toml manifest because `cargo_metadata` doesn't distinguish between
        // empty `version` in Cargo.toml and `version = "*"`
        let package_manifest = LocalManifest::try_new(&self.manifest_path)?;
        let package_dir = crate::manifest_dir(&package_manifest.path)?.to_owned();

        let mut deps_to_update: Vec<&Self> = vec![];
        for (p, next_ver) in updated_packages {
            let canonical_path = p.canonical_path()?;
            // Find the dependencies that have the same path as the updated package.
            let matching_deps = package_manifest
                .get_package_dependency_tables()
                .flat_map(|t| {
                    t.iter().filter_map(|(name, d)| {
                        d.as_table_like().map(|d| {
                            match workspace_dependencies {
                                Some(workspace_dependencies) if is_workspace_dependency(d) => {
                                    // The dependency of the package Cargo.toml is inherited from the workspace,
                                    // so we find the dependency of the workspace and use it instead.
                                    let dep = workspace_dependencies
                                        .iter()
                                        .find(|(n, _)| n == &name)
                                        .and_then(|(_, d)| d.as_table_like())
                                        .unwrap_or(d);
                                    // Return also the path of the Cargo.toml so that we can resolve the
                                    // relative path of the dependency later.
                                    (workspace_dir, dep)
                                }
                                _ => (package_dir.as_path(), d),
                            }
                        })
                    })
                })
                .filter(|(_toml_base_path, d)| include_versionless || d.contains_key("version"))
                .filter(|(toml_base_path, d)| {
                    crate::is_dependency_referred_to_package(*d, toml_base_path, &canonical_path)
                })
                .map(|(_, dep)| dep);

            for dep in matching_deps {
                if !dep.contains_key("version") || should_update_dependency(dep, next_ver)? {
                    deps_to_update.push(p);
                }
            }
        }

        Ok(deps_to_update)
    }
}

/// Check if the dependency is in the form of `dep_name.workspace = true`.
fn is_workspace_dependency(d: &dyn TableLike) -> bool {
    d.get("workspace")
        .is_some_and(|w| w.as_bool() == Some(true))
        && !d.contains_key("version")
        && !d.contains_key("path")
}

fn should_update_dependency(dep: &dyn TableLike, next_ver: &Version) -> anyhow::Result<bool> {
    let old_req = dep
        .get("version")
        .expect("versionless dependencies are handled by the caller")
        .as_str()
        .unwrap_or("*");
    let should_update_dep = cargo_utils::upgrade_requirement(old_req, next_ver)?.is_some();
    Ok(should_update_dep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_declarations_only_update_packages_that_use_them() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let root = directory.path();
        for (path, name, dependencies) in [
            ("", "root-app", ""),
            ("support", "support", ""),
            (
                "consumer",
                "consumer",
                "[dependencies]\nshared.workspace = true\n",
            ),
        ] {
            let package = root.join(path);
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
        let manifest_path = root.join("Cargo.toml");
        let package_manifest = fs_err::read_to_string(&manifest_path).unwrap();
        // Check both versionless Git-only dependencies and versioned dependencies.
        for (version, include_versionless) in [("", true), (", version = \"0.1\"", false)] {
            let workspace_manifest = format!(
                "{package_manifest}\n[workspace]\nmembers = [\"support\", \"consumer\"]\nresolver = \"2\"\n[workspace.dependencies]\nshared = {{ package = \"support\", path = \"support\"{version} }}\n"
            );
            // Actual root dependencies must still propagate, including renamed,
            // inherited dependencies and target-specific build/dev dependencies.
            for dependency in [
                "",
                "[dependencies]\nshared.workspace = true\n",
                "[dependencies]\nsupport = { path = \"support\", version = \"0.1\" }\n",
                "[target.'cfg(unix)'.build-dependencies]\nshared.workspace = true\n",
                "[dev-dependencies]\nshared.workspace = true\n",
            ] {
                fs_err::write(&manifest_path, format!("{workspace_manifest}{dependency}")).unwrap();
                let metadata = cargo_utils::get_manifest_metadata(&manifest_path).unwrap();
                let manifest = LocalManifest::try_new(&manifest_path).unwrap();
                let support = metadata
                    .packages
                    .iter()
                    .find(|p| p.name == "support")
                    .unwrap();
                let updated = [(support, Version::new(0, 2, 0))];
                for name in ["root-app", "consumer"] {
                    let package = metadata.packages.iter().find(|p| p.name == name).unwrap();
                    let dependencies = package
                        .dependencies_to_update(
                            &updated,
                            manifest.get_workspace_dependency_table(),
                            root,
                            include_versionless,
                        )
                        .unwrap();
                    let expected = if name == "consumer" || !dependency.is_empty() {
                        vec!["support"]
                    } else {
                        vec![]
                    };
                    assert_eq!(
                        dependencies
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>(),
                        expected,
                        "{name}: version={version:?}, dependency={dependency:?}"
                    );
                }
            }
        }
    }
}
