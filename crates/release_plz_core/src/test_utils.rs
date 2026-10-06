//! Helpers to scaffold Cargo packages on disk in unit tests.

use cargo_metadata::camino::Utf8Path;
use cargo_utils::CARGO_TOML;
use git_cmd::Repo;

/// The manifest written by [`write_package`]: a `[package]` table with `name`,
/// `version` and `edition = "2024"`, followed by `extra_toml`.
pub(crate) fn package_manifest(name: &str, version: &str, extra_toml: &str) -> String {
    format!("[package]\nname = {name:?}\nversion = {version:?}\nedition = \"2024\"\n{extra_toml}")
}

/// Write a minimal library package to `dir`: an empty `src/lib.rs` and the manifest
/// returned by [`package_manifest`].
pub(crate) fn write_package(dir: &Utf8Path, name: &str, version: &str, extra_toml: &str) {
    write_package_with_manifest(dir, &package_manifest(name, version, extra_toml));
}

/// Write a minimal library package to `dir`: an empty `src/lib.rs` and `manifest`,
/// which can express what [`package_manifest`] can't, such as an inherited version.
pub(crate) fn write_package_with_manifest(dir: &Utf8Path, manifest: &str) {
    fs_err::create_dir_all(dir.join("src")).unwrap();
    fs_err::write(dir.join("src/lib.rs"), "").unwrap();
    fs_err::write(dir.join(CARGO_TOML), manifest).unwrap();
}

/// Initialize a git repository in `dir` and commit the files written by
/// `write_files`, together with their lockfile, as the published baseline.
pub(crate) fn init_baseline_repo(dir: &Utf8Path, write_files: impl FnOnce(&Utf8Path)) -> Repo {
    fs_err::create_dir_all(dir).unwrap();
    let repo = Repo::init(dir);
    // Keep checked-out files byte-identical to the LF-only fixtures.
    repo.git(&["config", "core.autocrlf", "false"]).unwrap();
    write_files(dir);
    fs_err::write(dir.join(".gitignore"), "/target\n").unwrap();
    generate_lockfile(dir);
    repo.add_all_and_commit("chore: published baseline")
        .unwrap();
    repo
}

/// Run `cargo` with `args` in `root`, asserting that it succeeds.
pub(crate) fn run_cargo_unwrap(root: &Utf8Path, args: &[&str]) {
    let output = crate::cargo::run_cargo(root, args).unwrap();
    assert!(output.status.success(), "{}", output.stderr);
}

/// Generate the lockfile of the workspace at `root` without network access.
pub(crate) fn generate_lockfile(root: &Utf8Path) {
    run_cargo_unwrap(root, &["generate-lockfile", "--offline"]);
}
