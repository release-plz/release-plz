use std::iter;

use cargo_metadata::{
    Package,
    camino::Utf8Path,
    semver::{Version, VersionReq},
};
use cargo_utils::{DepKind, LocalManifest};
use toml_edit::TableLike;

use crate::PackagePath as _;

/// Policy for rewriting version requirements on local workspace dependencies.
///
/// The same decision must drive both dependent-release detection and manifest writes:
/// skipping only the write still schedules unnecessary releases, while skipping only
/// release detection changes manifests without releasing their dependent packages.
///
/// This affects dependent releases triggered by requirement changes. Other release rules
/// and the size of a dependent's version bump remain unchanged. Workspace-wide configuration
/// also gives inherited requirements a single policy, even when several packages share the
/// same workspace dependency.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LocalDependenciesUpdateStrategy {
    /// Rewrite requirements to the new version using their existing precision.
    #[default]
    Always,
    /// Preserve requirements that already accept the new version.
    ///
    /// For example, `0.6.7` accepts `0.6.8` while retaining the intentional `0.6.7` floor.
    /// Maintainers must raise that floor explicitly when they require a newer API or fix;
    /// consumers with older lockfiles are otherwise allowed to keep compatible versions.
    IfNeeded,
    /// Preserve all requirements, even when they reject the new version.
    ///
    /// Maintainers must update incompatible requirements before releasing. Versionless
    /// Git-only dependencies still propagate releases because there is no requirement to keep.
    Never,
}

impl LocalDependenciesUpdateStrategy {
    /// Rewrite references to a local package in member and workspace manifests.
    pub(super) fn update_dependencies(
        self,
        all_packages: &[&Package],
        version: &Version,
        package_path: &Utf8Path,
        workspace_manifest: &Utf8Path,
    ) -> anyhow::Result<()> {
        let all_manifests = iter::once(workspace_manifest)
            .chain(all_packages.iter().map(|pkg| pkg.manifest_path.as_path()));
        for manifest in all_manifests {
            let mut local_manifest = LocalManifest::try_new(manifest)?;
            let manifest_dir = crate::manifest_dir(&local_manifest.path)?.to_owned();
            let deps_to_update = local_manifest
                .get_dependency_tables_mut()
                .flat_map(|t| t.iter_mut().filter_map(|(_, d)| d.as_table_like_mut()))
                .filter(|d| d.contains_key("version"))
                .filter(|d| {
                    crate::is_dependency_referred_to_package(*d, &manifest_dir, package_path)
                });

            for dep in deps_to_update {
                let old_req = dep
                    .get("version")
                    .expect("filter ensures this")
                    .as_str()
                    .unwrap_or("*");
                if let Some(new_req) = self.upgrade_requirement(old_req, version)? {
                    dep.insert("version", toml_edit::value(new_req));
                }
            }
            local_manifest.write()?;
        }
        Ok(())
    }

    fn upgrade_requirement(
        self,
        requirement: &str,
        version: &Version,
    ) -> anyhow::Result<Option<String>> {
        if self == Self::Never
            || (self == Self::IfNeeded && VersionReq::parse(requirement)?.matches(version))
        {
            return Ok(None);
        }
        // Reuse the existing rewrite rules for requirements that need changing. In particular,
        // do not guess how to rewrite unsupported inequalities or compound ranges.
        cargo_utils::upgrade_requirement(requirement, version)
    }

    /// Find dependencies whose new versions require a dependent release.
    /// Git-only releases also propagate through versionless non-dev dependencies.
    pub(super) fn dependencies_to_update<'a>(
        self,
        package: &Package,
        updated_packages: &'a [(&Package, Version)],
        workspace_dependencies: Option<&dyn TableLike>,
        workspace_dir: &Utf8Path,
        include_versionless: bool,
    ) -> anyhow::Result<Vec<&'a Package>> {
        // Look into the toml manifest because `cargo_metadata` doesn't distinguish between
        // empty `version` in Cargo.toml and `version = "*"`
        let package_manifest = LocalManifest::try_new(&package.manifest_path)?;
        let package_dir = crate::manifest_dir(&package_manifest.path)?;

        let mut deps_to_update = vec![];
        for (p, next_ver) in updated_packages {
            let canonical_path = p.canonical_path()?;
            // Find the dependencies that have the same path as the updated package.
            // Dev dependencies are included on purpose: `should_update_dependency`
            // decides which of them count.
            let matching_deps = package_manifest
                .get_package_dependency_tables()
                .flat_map(|(kind, t)| {
                    t.iter().filter_map(move |(name, d)| {
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
                                    (kind, workspace_dir, dep)
                                }
                                _ => (kind, package_dir, d),
                            }
                        })
                    })
                })
                .filter(|(_, toml_base_path, d)| {
                    crate::is_dependency_referred_to_package(*d, toml_base_path, &canonical_path)
                })
                .map(|(kind, _, dep)| (kind, dep));

            for (kind, dep) in matching_deps {
                if should_update_dependency(dep, kind, next_ver, include_versionless, self)? {
                    deps_to_update.push(*p);
                    // A package can declare the same dependency in several tables
                    // (for example `[dependencies]` and `[dev-dependencies]`).
                    // It still needs a single release.
                    break;
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

/// Whether a dependency on an updated package means the dependent must be released.
///
/// A dependency with a version requirement counts when that requirement has to be
/// rewritten. A dependency without one has nothing to rewrite, so it only counts for
/// Git-only releases, which propagate through the path dependencies that are part of
/// the released package: `[dependencies]` and `[build-dependencies]`, but not
/// `[dev-dependencies]`, which only affect its tests.
fn should_update_dependency(
    dep: &dyn TableLike,
    kind: DepKind,
    next_ver: &Version,
    include_versionless: bool,
    policy: LocalDependenciesUpdateStrategy,
) -> anyhow::Result<bool> {
    let Some(old_req) = dep.get("version") else {
        return Ok(include_versionless && kind != DepKind::Development);
    };
    let old_req = old_req.as_str().unwrap_or("*");
    let should_update_dep = policy.upgrade_requirement(old_req, next_ver)?.is_some();
    Ok(should_update_dep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::write_package;

    #[test]
    fn compatible_requirements_keep_their_minimum_and_spelling() {
        for (requirement, version) in [
            ("0.6.7", "0.6.8"),
            ("^0.6.7", "0.6.8"),
            ("1.2.3", "1.3.0"),
            ("~1.2.3", "1.2.4"),
            ("=1.2.3", "1.2.3"),
            ("1.2.*", "1.2.4"),
            ("*", "2.0.0"),
            (">=1.2.3, <2.0.0", "1.3.0"),
            (">1.2.3", "1.2.4"),
            ("<=1.2.3", "1.2.3"),
            ("1.2.3-alpha.1", "1.2.3-alpha.2"),
            ("1.2.3-alpha.1", "1.2.3"),
        ] {
            let version = Version::parse(version).unwrap();
            assert_eq!(
                LocalDependenciesUpdateStrategy::IfNeeded
                    .upgrade_requirement(requirement, &version)
                    .unwrap(),
                None,
                "{requirement} -> {version}"
            );
        }
    }

    #[test]
    fn incompatible_requirements_are_rewritten() {
        for (requirement, version, expected) in [
            ("0.6.7", "0.7.0", "0.7.0"),
            ("0.0.7", "0.0.8", "0.0.8"),
            ("1.2.3", "2.0.0", "2.0.0"),
            ("^1.2.3", "2.0.0", "^2.0.0"),
            ("~1.2.3", "1.3.0", "~1.3.0"),
            ("=1.2.3", "1.2.4", "=1.2.4"),
            ("1.2.*", "1.3.0", "1.3.*"),
            ("1.2.3", "1.3.0-alpha.1", "1.3.0-alpha.1"),
            ("1.2.3-alpha.1", "1.3.0-alpha.1", "1.3.0-alpha.1"),
        ] {
            let version = Version::parse(version).unwrap();
            assert_eq!(
                LocalDependenciesUpdateStrategy::IfNeeded
                    .upgrade_requirement(requirement, &version)
                    .unwrap()
                    .as_deref(),
                Some(expected),
                "{requirement} -> {version}"
            );
        }
    }

    #[test]
    fn never_preserves_even_incompatible_requirements() {
        for requirement in ["0.6.7", "=0.6.7", ">=0.6.7, <0.7.0", "0.7.0-alpha.1"] {
            assert_eq!(
                LocalDependenciesUpdateStrategy::Never
                    .upgrade_requirement(requirement, &Version::new(1, 0, 0))
                    .unwrap(),
                None,
                "{requirement}"
            );
        }
    }

    #[test]
    fn never_preserves_incompatible_manifests_without_scheduling_releases() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let root = directory.path();
        write_package(
            root,
            "consumer",
            "0.1.0",
            "[dependencies]\nsupport = { path = \"support\", version = \"=0.1.0\" }\n[workspace]\nmembers = [\"support\"]\n",
        );
        write_package(&root.join("support"), "support", "0.1.0", "");
        let manifest_path = root.join("Cargo.toml");
        let original = fs_err::read_to_string(&manifest_path).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest_path).unwrap();
        let consumer = metadata
            .packages
            .iter()
            .find(|p| p.name == "consumer")
            .unwrap();
        let support = metadata
            .packages
            .iter()
            .find(|p| p.name == "support")
            .unwrap();
        let next_version = Version::new(0, 2, 0);
        let strategy = LocalDependenciesUpdateStrategy::Never;
        let updated = [(support, next_version.clone())];
        assert!(
            strategy
                .dependencies_to_update(consumer, &updated, None, root, false)
                .unwrap()
                .is_empty()
        );
        strategy
            .update_dependencies(
                &[consumer],
                &next_version,
                &support.canonical_path().unwrap(),
                &manifest_path,
            )
            .unwrap();
        assert_eq!(fs_err::read_to_string(manifest_path).unwrap(), original);
    }

    #[test]
    fn never_still_propagates_versionless_git_only_dependencies() {
        let dependency: toml_edit::DocumentMut = "path = \"support\"".parse().unwrap();
        for (kind, include_versionless, expected) in [
            (DepKind::Normal, true, true),
            (DepKind::Build, true, true),
            (DepKind::Development, true, false),
            (DepKind::Normal, false, false),
        ] {
            assert_eq!(
                should_update_dependency(
                    dependency.as_table(),
                    kind,
                    &Version::new(0, 2, 0),
                    include_versionless,
                    LocalDependenciesUpdateStrategy::Never
                )
                .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn default_still_raises_compatible_minimums() {
        assert_eq!(
            LocalDependenciesUpdateStrategy::default()
                .upgrade_requirement("0.6.7", &Version::new(0, 6, 8))
                .unwrap()
                .as_deref(),
            Some("0.6.8")
        );
    }

    #[test]
    fn unsupported_incompatible_ranges_still_error() {
        assert!(
            LocalDependenciesUpdateStrategy::IfNeeded
                .upgrade_requirement(">=1.2.3, <2.0.0", &Version::new(2, 0, 0))
                .is_err()
        );
    }

    #[test]
    fn versionless_and_workspace_dependencies_to_update() {
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
            write_package(&root.join(path), name, "0.1.0", dependencies);
        }
        let manifest_path = root.join("Cargo.toml");
        let package_manifest = fs_err::read_to_string(&manifest_path).unwrap();
        // Check both versionless Git-only dependencies and versioned dependencies.
        for (version, include_versionless) in [("", true), (", version = \"0.1\"", false)] {
            let workspace_manifest = format!(
                "{package_manifest}\n[workspace]\nmembers = [\"support\", \"consumer\"]\nresolver = \"3\"\n[workspace.dependencies]\nshared = {{ package = \"support\", path = \"support\"{version} }}\n"
            );
            // Actual root dependencies must still propagate, including renamed,
            // inherited dependencies and target-specific build/dev dependencies.
            for (dependency, dev_only) in [
                ("", false),
                ("[dependencies]\nshared.workspace = true\n", false),
                (
                    "[dependencies]\nsupport = { path = \"support\", version = \"0.1\" }\n",
                    false,
                ),
                (
                    "[target.'cfg(unix)'.build-dependencies]\nshared.workspace = true\n",
                    false,
                ),
                // A versioned dev dependency is rewritten, so it always propagates.
                (
                    "[dev-dependencies]\nsupport = { path = \"support\", version = \"0.1\" }\n",
                    false,
                ),
                // A versionless dev dependency doesn't change the released package.
                ("[dev-dependencies]\nshared.workspace = true\n", true),
                (
                    "[target.'cfg(unix)'.dev-dependencies]\nshared.workspace = true\n",
                    true,
                ),
                // Unless the package also uses it outside of its tests.
                (
                    "[dependencies]\nshared.workspace = true\n[dev-dependencies]\nshared.workspace = true\n",
                    false,
                ),
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
                    let dependencies = LocalDependenciesUpdateStrategy::Always
                        .dependencies_to_update(
                            package,
                            &updated,
                            manifest.get_workspace_dependency_table(),
                            root,
                            include_versionless,
                        )
                        .unwrap();
                    // Only a versionless dev-only declaration of the updated package
                    // leaves the root package alone.
                    let root_app_updated =
                        !dependency.is_empty() && !(dev_only && version.is_empty());
                    let expected = if name == "consumer" || root_app_updated {
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
