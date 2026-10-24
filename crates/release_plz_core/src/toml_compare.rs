use cargo_metadata::{Dependency, DependencyKind, Source};

/// Compare the dependencies of the registry package and the local one.
/// Check if the dependencies of the registry package were updated.
/// This function checks only dependencies of `Cargo.toml`.
///
/// Local path dependencies and dev-dependencies are ignored.
/// Dev-dependencies are only used by tests, examples and benchmarks, so changing them
/// doesn't change what the users of the package build.
pub fn are_toml_dependencies_updated(
    registry_dependencies: &[Dependency],
    local_dependencies: &[Dependency],
) -> bool {
    local_dependencies.iter().any(|d| {
        d.path.is_none()
            && d.kind != DependencyKind::Development
            && !is_dependency_in_registry_dependencies(d, registry_dependencies)
    })
}

/// Check if a local dependency is in the registry dependencies.
/// If not, it means that the Cargo.toml file was updated since the last release.
fn is_dependency_in_registry_dependencies(
    local_dep: &Dependency,
    registry_dependencies: &[Dependency],
) -> bool {
    registry_dependencies
        .iter()
        .any(|registry_dep| are_dependencies_equal(local_dep, registry_dep))
}

fn are_dependencies_equal(local_dep: &Dependency, registry_dep: &Dependency) -> bool {
    local_dep.name == registry_dep.name
        && local_dep.req == registry_dep.req
        && local_dep.kind == registry_dep.kind
        && local_dep.optional == registry_dep.optional
        && local_dep.uses_default_features == registry_dep.uses_default_features
        && local_dep.features == registry_dep.features
        && local_dep.target == registry_dep.target
        && local_dep.rename == registry_dep.rename
        && local_dep.registry == registry_dep.registry
        && is_source_equal(local_dep.source.as_ref(), registry_dep.source.as_ref())
}

fn is_source_equal(local_source: Option<&Source>, registry_source: Option<&Source>) -> bool {
    match (local_source, registry_source) {
        (Some(local_source), Some(registry_source)) => {
            // If the source is a git repository, we don't check the source of the registry, because
            // you can't publish a git repository on crates.io.
            if local_source.to_string().starts_with("git+") {
                true
            } else {
                local_source == registry_source
            }
        }
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dependency(name: &str, req: &str, kind: &str) -> Dependency {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "source": null,
            "req": req,
            "kind": kind,
            "optional": false,
            "uses_default_features": true,
            "features": [],
            "target": null,
            "rename": null,
            "registry": null,
            "path": null,
        }))
        .unwrap()
    }

    #[test]
    fn unchanged_dependencies_are_not_reported_as_updated() {
        let registry = [dependency("serde", "^1.0", "normal")];
        let local = [dependency("serde", "^1.0", "normal")];
        assert!(!are_toml_dependencies_updated(&registry, &local));
    }

    #[test]
    fn dev_dependency_requirement_change_is_ignored() {
        let registry = [dependency("tempfile", "^3.0", "dev")];
        let local = [dependency("tempfile", "^3.10", "dev")];
        assert!(!are_toml_dependencies_updated(&registry, &local));
    }

    #[test]
    fn added_dev_dependency_is_ignored() {
        let registry = [];
        let local = [dependency("tempfile", "^3.0", "dev")];
        assert!(!are_toml_dependencies_updated(&registry, &local));
    }

    #[test]
    fn normal_dependency_requirement_change_is_reported() {
        let registry = [dependency("serde", "^1.0", "normal")];
        let local = [dependency("serde", "^1.0.200", "normal")];
        assert!(are_toml_dependencies_updated(&registry, &local));
    }

    #[test]
    fn dependency_moved_from_dev_to_normal_is_reported() {
        let registry = [dependency("serde", "^1.0", "dev")];
        let local = [dependency("serde", "^1.0", "normal")];
        assert!(are_toml_dependencies_updated(&registry, &local));
    }

    #[test]
    fn build_dependency_change_is_reported() {
        let registry = [dependency("cc", "^1.0", "build")];
        let local = [dependency("cc", "^1.1", "build")];
        assert!(are_toml_dependencies_updated(&registry, &local));
    }
}
