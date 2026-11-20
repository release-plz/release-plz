use cargo_metadata::camino::Utf8PathBuf;

#[must_use]
pub fn fake_metadata() -> cargo_metadata::Metadata {
    cargo_utils::get_manifest_metadata(&Utf8PathBuf::from("Cargo.toml")).unwrap()
}
