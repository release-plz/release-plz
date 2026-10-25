use cargo_metadata::{Dependency, Source};

/// Compare the dependencies of the registry package and the local one.
/// Check if the dependencies of the registry package were updated.
/// This function checks only dependencies of `Cargo.toml`.
pub fn are_toml_dependencies_updated(
    registry_dependencies: &[Dependency],
    local_dependencies: &[Dependency],
) -> bool {
    local_dependencies.iter().any(|d| {
        d.path.is_none() && !is_dependency_in_registry_dependencies(d, registry_dependencies)
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
