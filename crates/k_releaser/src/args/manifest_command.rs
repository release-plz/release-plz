use std::path::Path;

use anyhow::Context as _;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use k_releaser_core::fs_utils::to_utf8_path;

/// Command that acts on a manifest.
pub trait ManifestCommand {
    fn optional_manifest(&self) -> Option<&Path>;

    fn optional_manifest_path(&self) -> Option<&Utf8Path> {
        self.optional_manifest().map(|p| to_utf8_path(p).unwrap())
    }

    fn manifest_path(&self) -> Utf8PathBuf {
        super::local_manifest(self.optional_manifest_path())
    }

    fn cargo_metadata(&self) -> anyhow::Result<cargo_metadata::Metadata> {
        let manifest = &self.manifest_path();
        cargo_utils::get_manifest_metadata(manifest).map_err(|e| match e {
            cargo_metadata::Error::CargoMetadata { stderr } => {
                let stderr = stderr.trim();
                anyhow::anyhow!("{stderr}. Use --manifest-path to specify the path to the manifest file if it's not in the current directory.")
            }
            _ => {
                anyhow::anyhow!(e)
            }
        }).with_context(|| format!("Failed to read metadata from manifest at {manifest}"))
    }
}
