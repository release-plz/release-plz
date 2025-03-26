use cargo_metadata::{Package, camino::Utf8Path, semver::Version};
use cargo_utils::LocalManifest;
use toml_edit::TableLike;

use crate::PackagePath as _;

pub trait PackageDependencies {
    /// Returns the `updated_packages` which should be updated in the dependencies of the package.
    fn dependencies_to_update<'a>(
        &self,
        updated_packages: &'a [(&Package, Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
    ) -> anyhow::Result<Vec<&'a Package>>;
}

impl PackageDependencies for Package {
    fn dependencies_to_update<'a>(
        &self,
        updated_packages: &'a [(&Package, Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
    ) -> anyhow::Result<Vec<&'a Package>> {
        // Look into the toml manifest because `cargo_metadata` doesn't distinguish between
        // empty `version` in Cargo.toml and `version = "*"`
        let package_manifest = LocalManifest::try_new(&self.manifest_path)?;
        let package_dir = crate::manifest_dir(&package_manifest.path)?.to_owned();

        let mut deps_to_update: Vec<&Package> = vec![];
        for (p, next_ver) in updated_packages {
            let canonical_path = p.canonical_path()?;
            // Find the dependencies that have the same path as the updated package.
            let matching_deps = package_manifest
                .get_dependency_tables()
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
                // Exclude path dependencies without `version`.
                .filter(|(_toml_base_path, d)| d.contains_key("version"))
                .filter(|(toml_base_path, d)| {
                    crate::is_dependency_referred_to_package(*d, toml_base_path, &canonical_path)
                })
                .map(|(_, dep)| dep);

            for dep in matching_deps {
                if should_update_dependency(dep, next_ver)? {
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
        .expect("filter ensures this")
        .as_str()
        .unwrap_or("*");
    let should_update_dep = cargo_utils::upgrade_requirement(old_req, next_ver)?.is_some();
    Ok(should_update_dep)
}
