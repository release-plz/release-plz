use assert_cmd::Command;
use cargo_metadata::camino::Utf8Path;

pub fn release_plz_cmd(target_dir: &Utf8Path) -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!(env!("CARGO_PKG_NAME"));
    // Run tests in isolation to avoid flakiness
    cmd.env("CARGO_TARGET_DIR", target_dir.as_str());
    cmd.env("RELEASE_PLZ_NO_ANSI", "1");
    // release-plz runs cargo on the fixture packages, e.g. `cargo publish`.
    allow_cargo_warnings(&mut cmd);
    cmd
}

pub fn cargo_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    allow_cargo_warnings(&mut cmd);
    cmd
}

/// Fixture packages declare unused dependencies, which cargo warns about.
/// Don't let `build.warnings = "deny"`, set by the caller's environment or cargo config
/// (e.g. CI), turn these warnings into errors.
fn allow_cargo_warnings(cmd: &mut Command) {
    cmd.env("CARGO_BUILD_WARNINGS", "warn");
}
