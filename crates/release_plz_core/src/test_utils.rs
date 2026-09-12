//! Helpers to scaffold Cargo packages on disk in unit tests.

use cargo_metadata::camino::Utf8Path;
use cargo_utils::CARGO_TOML;

/// The manifest written by [`write_package`]: a `[package]` table with `name`,
/// `version` and `edition = "2021"`, followed by `extra_toml`.
pub(crate) fn package_manifest(name: &str, version: &str, extra_toml: &str) -> String {
    format!("[package]\nname = {name:?}\nversion = {version:?}\nedition = \"2021\"\n{extra_toml}")
}

/// Write a minimal library package to `dir`: an empty `src/lib.rs` and the manifest
/// returned by [`package_manifest`].
pub(crate) fn write_package(dir: &Utf8Path, name: &str, version: &str, extra_toml: &str) {
    fs_err::create_dir_all(dir.join("src")).unwrap();
    fs_err::write(dir.join("src/lib.rs"), "").unwrap();
    fs_err::write(
        dir.join(CARGO_TOML),
        package_manifest(name, version, extra_toml),
    )
    .unwrap();
}
