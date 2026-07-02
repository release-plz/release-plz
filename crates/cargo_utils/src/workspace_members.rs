use std::collections::BTreeSet;

use cargo_metadata::{Metadata, Package};

/// Lookup all members of the current workspace
pub fn workspace_members(metadata: &Metadata) -> anyhow::Result<impl Iterator<Item = Package>> {
    let workspace_members: BTreeSet<_> = metadata.workspace_members.clone().into_iter().collect();
    let workspace_members = metadata
        .packages
        .clone()
        .into_iter()
        .filter(move |p| workspace_members.contains(&p.id))
        .map(|mut p| {
            p.manifest_path = canonicalize_path(p.manifest_path);
            for dep in &mut p.dependencies {
                dep.path = dep.path.take().map(canonicalize_path);
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
