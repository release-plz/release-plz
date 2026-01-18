use cargo_metadata::camino::Utf8PathBuf;
use std::env;

pub fn fake_metadata() -> cargo_metadata::Metadata {
    // In tests, CARGO_MANIFEST_DIR points to the crate being tested
    // We need to go up to the workspace root
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|_| Utf8PathBuf::from("."));

    // Try workspace root first (go up until we find workspace Cargo.toml)
    let mut current = manifest_dir.clone();
    for _ in 0..5 {
        let workspace_toml = current.join("Cargo.toml");
        if workspace_toml.exists() {
            if let Ok(metadata) = cargo_utils::get_manifest_metadata(&workspace_toml) {
                return metadata;
            }
        }
        if !current.pop() {
            break;
        }
    }

    // Fallback to current directory
    cargo_utils::get_manifest_metadata(&Utf8PathBuf::from("Cargo.toml"))
        .expect("Failed to get cargo metadata. Ensure Cargo.toml exists in workspace root.")
}
