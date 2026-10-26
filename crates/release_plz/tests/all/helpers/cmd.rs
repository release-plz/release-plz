use assert_cmd::{Command, assert::Assert};
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
pub fn run_with_debug_logs(command: Command) -> String {
    debug_logs(command, Assert::success)
}

/// Run the release-plz `command`, asserting that it fails, and return its
/// debug logs, followed by its error.
pub fn fail_with_debug_logs(command: Command) -> String {
    debug_logs(command, Assert::failure)
}

/// Run the release-plz `command`, asserting its `outcome`, and return its
/// standard error, with debug logs.
fn debug_logs(mut command: Command, outcome: fn(Assert) -> Assert) -> String {
    let assert = command
        .env("RELEASE_PLZ_LOG", "release_plz_core=debug")
        .assert();
    String::from_utf8(outcome(assert).get_output().stderr.clone()).unwrap()
}
