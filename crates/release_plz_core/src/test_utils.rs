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

/// Run `cargo` with `args` in `root`, asserting that it succeeds.
pub(crate) fn run_cargo_ok(root: &Utf8Path, args: &[&str]) {
    let output = crate::cargo::run_cargo(root, args).unwrap();
    assert!(output.status.success(), "{}", output.stderr);
}

/// Generate the lockfile of the workspace at `root` without network access.
pub(crate) fn generate_lockfile(root: &Utf8Path) {
    run_cargo_ok(root, &["generate-lockfile", "--offline"]);
}
