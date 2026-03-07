use cargo_metadata::{Dependency, DependencyKind, Package};
use tracing::debug;

/// Return packages in an order they can be released.
/// In the result, the packages are placed after all their dependencies.
/// Return an error if a circular dependency is detected.
pub fn release_order<'a>(packages: &'a [&Package]) -> anyhow::Result<Vec<&'a Package>> {
    let mut order = vec![];
    let mut passed = vec![];
    for p in packages {
        release_order_inner(packages, p, &mut order, &mut passed)?;
    }
    debug!(
        "Release order: {:?}",
        order.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    Ok(order)
}

/// The `passed` argument is used to track packages that you already visited to
/// detect circular dependencies.
fn release_order_inner<'a>(
    packages: &[&'a Package],
    pkg: &'a Package,
    order: &mut Vec<&'a Package>,
    passed: &mut Vec<&'a Package>,
) -> anyhow::Result<()> {
    if is_package_in(pkg, order) {
        return Ok(());
    }
    passed.push(pkg);

    for d in &pkg.dependencies {
        // Check if the dependency is part of the packages we are releasing.
        if let Some(dep) = packages.iter().find(|p| {
            d.name == *p.name
              // Exclude the current package.
              && p.name != pkg.name
              && should_dep_be_released_before(d)
        }) {
            anyhow::ensure!(
                !is_package_in(dep, passed),
                "Circular dependency detected: {} -> {}",
                dep.name,
                pkg.name,
            );
            release_order_inner(packages, dep, order, passed)?;
        }
    }

    order.push(pkg);
    passed.clear();
    Ok(())
}

/// Return true if the package is part of a packages array.
/// This function exists because `package.contains(pkg)` is expensive,
/// because it compares the whole package struct.
fn is_package_in(pkg: &Package, packages: &[&Package]) -> bool {
    packages.iter().any(|p| p.name == pkg.name)
}

/// Check if the dependency should be released before the current package.
/// Normal and Build dependencies always affect release order.
/// Dev-dependencies affect release order only if they have a version
/// requirement specified — matching `cargo publish --workspace` behavior.
/// Dev-deps without a version (`req == "*"`) are stripped from the published
/// manifest, so they don't need to be published first.
fn should_dep_be_released_before(dep: &Dependency) -> bool {
    match dep.kind {
        DependencyKind::Normal | DependencyKind::Build => true,
        // Dev-deps with a version requirement remain in the published manifest
        // and must be resolvable on the registry. Dev-deps without a version
        // (req == "*") are stripped by cargo on publish, so they can be ignored.
        DependencyKind::Development => !dep.req.comparators.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use fake_package::{FakeDependency, FakePackage};

    use super::*;
    use crate::publishable_packages_from_manifest;

    // Test the package release order in the release-plz workspace itself.
    #[test]
    fn workspace_release_order_is_correct() {
        let public_packages = publishable_packages_from_manifest("../../Cargo.toml").unwrap();
        let pkgs = &public_packages.iter().collect::<Vec<_>>();
        assert_eq!(
            order(pkgs),
            [
                "cargo_utils",
                "git_cmd",
                "test_logs",
                "next_version",
                "release_plz_core",
                "release-plz"
            ]
        );
    }

    /// Package
    fn pkg(name: &str, deps: &[FakeDependency]) -> Package {
        FakePackage::new(name)
            .with_dependencies(deps.to_vec())
            .into()
    }

    /// Dependency
    fn dep(name: &str) -> FakeDependency {
        FakeDependency::new(name)
    }

    /// Development dependency
    fn dev_dep(name: &str) -> FakeDependency {
        FakeDependency::new(name).dev()
    }

    fn order<'a>(pkgs: &'a [&'a Package]) -> Vec<&'a str> {
        release_order(pkgs)
            .unwrap()
            .iter()
            .map(|p| p.name.as_str())
            .collect()
    }

    // Diagrams created with https://asciiflow.com/

    /// A─►B
    #[test]
    fn single_package_is_returned() {
        let pkgs = [&pkg("a", &[dep("b")])];
        assert_eq!(order(&pkgs), ["a"]);
    }

    /// ┌──┐
    /// │  ▼
    /// A  B
    /// ▲  │
    /// └──┘
    #[test]
    fn two_packages_cycle_is_detected() {
        let pkgs = [&pkg("a", &[dep("b")]), &pkg("b", &[dep("a")])];
        expect_test::expect!["Circular dependency detected: a -> b"]
            .assert_eq(&release_order(&pkgs).unwrap_err().to_string());
    }

    /// ┌──┐
    /// │  ▼
    /// A  B (versioned dev dependency)
    /// ▲  │
    /// └──┘
    /// Versioned dev-dep cycles are circular dependencies because
    /// `cargo package` validates versioned dev-deps against the registry,
    /// so neither package can be published first.
    #[test]
    fn two_packages_versioned_dev_cycle_is_detected() {
        let pkgs = [&pkg("a", &[dev_dep("b")]), &pkg("b", &[dep("a")])];
        expect_test::expect!["Circular dependency detected: a -> b"]
            .assert_eq(&release_order(&pkgs).unwrap_err().to_string());
    }

    /// ┌──┐
    /// │  ▼
    /// A  B (unversioned dev dependency)
    /// ▲  │
    /// └──┘
    /// Unversioned dev-deps are stripped from the published manifest,
    /// so they don't create a real cycle.
    #[test]
    fn two_packages_unversioned_dev_cycle_is_ok() {
        let pkgs = [
            &pkg("a", &[FakeDependency::new("b").unversioned_dev()]),
            &pkg("b", &[dep("a")]),
        ];
        assert_eq!(order(&pkgs), ["a", "b"]);
    }

    /// ┌─────┐
    /// ▼     │
    /// A────►B
    /// │     ▲
    /// └─►C──┘
    #[test]
    fn three_packages_cycle_is_detected() {
        let pkgs = [
            &pkg("a", &[dep("b")]),
            &pkg("a", &[dep("c")]),
            &pkg("b", &[dep("a")]),
            &pkg("c", &[dep("b")]),
        ];
        expect_test::expect!["Circular dependency detected: a -> b"]
            .assert_eq(&release_order(&pkgs).unwrap_err().to_string());
    }

    /// A────►C
    /// │     ▲
    /// └─►B──┘
    #[test]
    fn three_packages_are_ordered() {
        let pkgs = [
            &pkg("a", &[dep("b")]),
            &pkg("b", &[dep("c")]),
            &pkg("c", &[]),
        ];
        assert_eq!(order(&pkgs), ["c", "b", "a"]);
    }

    /// ┌──┐
    /// │  ▼
    /// A  B (dev dependency)
    /// ▲  │
    /// └──┘
    #[test]
    fn two_packages_dev_cycle_with_package_in_features_is_detected() {
        let mut a = pkg("a", &[dev_dep("b")]);
        a.features = [("my_feat".to_string(), vec!["b/feat".to_string()])].into();
        let pkgs = [&a, &pkg("b", &[dep("a")])];
        expect_test::expect!["Circular dependency detected: a -> b"]
            .assert_eq(&release_order(&pkgs).unwrap_err().to_string());
    }

    /// ┌──┐
    /// │  ▼
    /// A  B (dev dependency)
    /// ▲  │
    /// └──┘
    #[test]
    fn two_packages_dev_cycle_with_random_feature_is_detected() {
        let mut a = pkg("a", &[dev_dep("b")]);
        a.features = [(
            "my_feat".to_string(),
            vec!["b".to_string(), "rand/b".to_string()],
        )]
        .into();
        let pkgs = [&a, &pkg("b", &[dep("a")])];
        expect_test::expect!["Circular dependency detected: a -> b"]
            .assert_eq(&release_order(&pkgs).unwrap_err().to_string());
    }

    /// A (versioned dev-dep on B) ──► B (no deps)
    /// `cargo package` validates versioned dev-deps against the registry,
    /// so B must be published before A.
    #[test]
    fn versioned_dev_dep_is_released_before_dependent() {
        let pkgs = [&pkg("a", &[dev_dep("b")]), &pkg("b", &[])];
        assert_eq!(order(&pkgs), ["b", "a"]);
    }

    /// A (unversioned dev-dep on B) ──► B (no deps)
    /// Unversioned dev-deps are stripped from the published manifest,
    /// so they don't constrain release order.
    #[test]
    fn unversioned_dev_dep_does_not_affect_order() {
        let pkgs = [
            &pkg("a", &[FakeDependency::new("b").unversioned_dev()]),
            &pkg("b", &[]),
        ];
        // Order matches input order since there's no dependency edge.
        assert_eq!(order(&pkgs), ["a", "b"]);
    }
}
