use assert_cmd::Command;
use cargo_metadata::camino::Utf8Path;

pub fn release_plz_cmd(target_dir: &Utf8Path) -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!(env!("CARGO_PKG_NAME"));
    // Run tests in isolation to avoid flakiness
    cmd.env("CARGO_TARGET_DIR", target_dir.as_str());
    cmd.env("RELEASE_PLZ_NO_ANSI", "1");
    cmd
}
