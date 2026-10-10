use cargo_metadata::{Package, camino::Utf8Path, semver::Version};
use cargo_utils::LocalManifest;

// Generated from the production inheritance-discovery block, including the
// workspace-version read previously performed by new_workspace_version.
fn inheritance_scan<'a>(workspace_packages: &'a [Package], local_manifest_path: &Utf8Path)
    -> anyhow::Result<(Option<Version>, Vec<&'a Package>)> {
        let mut inheriting_packages = Vec::new();
        for package in workspace_packages {
            if LocalManifest::try_new(&package.manifest_path)?.version_is_inherited() {
                inheriting_packages.push(package);
            }
        }
        let workspace_version = {
            let local_manifest = LocalManifest::try_new(local_manifest_path)?;
            local_manifest.get_workspace_version()
        };
    Ok((workspace_version, inheriting_packages))
}
