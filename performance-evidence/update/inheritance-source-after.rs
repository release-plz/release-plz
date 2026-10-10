use cargo_metadata::{Package, camino::Utf8Path, semver::Version};
use cargo_utils::LocalManifest;

// Generated from the production inheritance-discovery block, including the
// workspace-version read previously performed by new_workspace_version.
fn inheritance_scan<'a>(workspace_packages: &'a [Package], local_manifest_path: &Utf8Path)
    -> anyhow::Result<(Option<Version>, Vec<&'a Package>)> {
        let (workspace_version, is_workspace_root) = {
            let manifest = LocalManifest::try_new(local_manifest_path)?;
            (manifest.get_workspace_version(), manifest.data.contains_key("workspace"))
        };
        let mut inheriting_packages = Vec::new();
        // A workspace root without a shared version cannot have inheriting members.
        // Still scan if a library caller supplied a package manifest instead.
        if workspace_version.is_some() || !is_workspace_root {
            for package in workspace_packages {
                if LocalManifest::try_new(&package.manifest_path)?.version_is_inherited() {
                    inheriting_packages.push(package);
                }
            }
        }
    Ok((workspace_version, inheriting_packages))
}
