use std::collections::{BTreeSet, HashMap};

use cargo_metadata::{Metadata, Package};

/// Lookup all members of the current workspace
pub fn workspace_members(metadata: &Metadata) -> anyhow::Result<impl Iterator<Item = Package>> {
    let workspace_members: BTreeSet<_> = metadata.workspace_members.iter().collect();
    // The same local dependency is often used by several workspace members.
    let mut dependency_paths = HashMap::new();
    let workspace_members = metadata
        .packages
        .iter()
        .filter(move |p| workspace_members.contains(&p.id))
        .cloned()
        .map(move |mut p| {
            p.manifest_path = canonicalize_path(p.manifest_path);
            for dep in &mut p.dependencies {
                dep.path = dep.path.take().map(|path| {
                    dependency_paths
                        .entry(path)
                        .or_insert_with_key(|path| canonicalize_path(path.clone()))
                        .clone()
                });
            }
            p
        });
    Ok(workspace_members)
}

fn canonicalize_path(
    path: cargo_metadata::camino::Utf8PathBuf,
) -> cargo_metadata::camino::Utf8PathBuf {
    if let Ok(path) = dunce::canonicalize(&path)
        && let Ok(path) = path.try_into()
    {
        return path;
    }

    path
}
