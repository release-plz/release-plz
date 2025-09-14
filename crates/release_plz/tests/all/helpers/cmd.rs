use assert_cmd::Command;

pub fn release_plz_cmd() -> Command {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    // Run tests in isolation to avoid flakiness
    cmd.env("CARGO_TARGET_DIR", "target");
    cmd
}
