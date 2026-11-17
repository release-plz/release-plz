use cargo_metadata::camino::Utf8PathBuf;

/// # Panics
///
/// Panics if Cargo cannot load metadata for the current directory's manifest.
#[must_use]
pub fn fake_metadata() -> cargo_metadata::Metadata {
    cargo_utils::get_manifest_metadata(&Utf8PathBuf::from("Cargo.toml")).unwrap()
}
