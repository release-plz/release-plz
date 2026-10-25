use std::path::Path;

use anyhow::Context as _;
use cargo_metadata::camino::Utf8PathBuf;
use cargo_utils::CARGO_TOML;
use release_plz_core::fs_utils::{current_directory, to_utf8_path};

/// Command that acts on a manifest.
pub trait ManifestCommand {
    fn optional_manifest(&self) -> Option<&Path>;

    fn manifest_path(&self) -> Utf8PathBuf {
        match self.optional_manifest() {
            Some(manifest) => to_utf8_path(manifest).unwrap().to_path_buf(),
            None => current_directory().unwrap().join(CARGO_TOML),
        }
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
