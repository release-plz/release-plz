use assert_cmd::Command;
use cargo_metadata::camino::Utf8Path;

pub fn release_plz_cmd(target_dir: &Utf8Path) -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!(env!("CARGO_PKG_NAME"));
    // Run tests in isolation to avoid flakiness
    cmd.env("CARGO_TARGET_DIR", target_dir.as_str());
    cmd.env("RELEASE_PLZ_NO_ANSI", "1");
    cmd
}

/// Run the release-plz `command`, asserting that it succeeds, and return its
/// debug logs.
pub fn run_with_debug_logs(mut command: Command) -> String {
    let output = command
        .env("RELEASE_PLZ_LOG", "release_plz_core=debug")
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    String::from_utf8(output).unwrap()
}
