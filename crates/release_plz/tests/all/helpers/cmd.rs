use assert_cmd::Command;

pub fn release_plz_cmd() -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!(env!("CARGO_PKG_NAME"));
    // Run tests in isolation to avoid flakiness
    cmd.env("CARGO_TARGET_DIR", "target");
    cmd.env("RELEASE_PLZ_NO_ANSI", "1");
    cmd
}
