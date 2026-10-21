use cargo_metadata::{Metadata, camino::Utf8Path};

pub mod cmd;
mod fake_utils;
pub mod gitea;
pub mod package;
mod reqwest_utils;
pub mod test_context;

pub const TEST_REGISTRY: &str = "test-registry";
pub const TEST_SPARSE_REGISTRY: &str = "test-registry-sparse";

pub fn today() -> String {
    // The changelogs specify the release date in UTC.
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

pub fn assert_cargo_semver_checks_is_installed() {
    assert!(
        release_plz_core::semver_check::is_cargo_semver_checks_installed(),
        "cargo-semver-checks is not installed. Please install it to run tests: https://github.com/obi1kenobi/cargo-semver-checks"
    );
}

/// Check resolved versions with `--locked`, which also rejects stale lockfiles.
pub fn assert_locked_versions(project_dir: &Utf8Path, expected_versions: &[(&str, &str)]) {
    let metadata = locked_metadata(project_dir);
    for (name, version) in expected_versions {
        let package = metadata.packages.iter().find(|p| p.name == *name).unwrap();
        assert_eq!(package.version.to_string(), *version, "package: {name}");
    }
}

/// Set the mode of the file at `path` to `0o755`, making it executable.
#[cfg(unix)]
pub fn make_executable(path: &Utf8Path) {
    use std::os::unix::fs::PermissionsExt as _;

    fs_err::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn locked_metadata(project_dir: &Utf8Path) -> Metadata {
    cargo_metadata::MetadataCommand::new()
        .current_dir(project_dir)
        .other_options(vec!["--locked".to_string(), "--offline".to_string()])
        .exec()
        .unwrap()
}
