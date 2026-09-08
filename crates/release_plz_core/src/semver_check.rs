use std::process::{Command, Output};

use anyhow::Context;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_utils::CARGO_TOML;

fn target_dir(path: &Utf8Path) -> Utf8PathBuf {
    path.join("target")
}

fn cargo_lock(path: &Utf8Path) -> Utf8PathBuf {
    path.join("Cargo.lock")
}

pub fn is_cargo_semver_checks_installed() -> bool {
    Command::new("cargo-semver-checks")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Outcome of semver check.
#[derive(Debug, Clone)]
pub enum SemverCheck {
    /// Semver check done. No incompatibilities found.
    Compatible,
    /// Semver check done. Incompatibilities found.
    Incompatible(String),
    /// Semver check skipped. This is the expected state for binaries.
    Skipped,
}

impl SemverCheck {
    pub fn outcome_str(&self) -> &'static str {
        match self {
            Self::Compatible => " (✓ API compatible changes)",
            Self::Incompatible(_) => " (⚠️ API breaking changes)",
            Self::Skipped => "",
        }
    }
}

pub fn run_semver_check(
    local_package: &Utf8Path,
    registry_package: &Utf8Path,
) -> anyhow::Result<SemverCheck> {
    let local_cargo_lock = cargo_lock(local_package);
    let registry_cargo_lock = cargo_lock(registry_package);
    let local_target_dir = target_dir(local_package);
    let registry_target_dir = target_dir(registry_package);

    let local_package_contained_cargo_lock = local_cargo_lock.exists();
    let registry_package_contained_cargo_lock = registry_cargo_lock.exists();
    let local_package_contained_target = local_target_dir.exists();
    let registry_package_contained_target = registry_target_dir.exists();

    let output = Command::new("cargo-semver-checks")
        .args(["semver-checks", "check-release"])
        // Only changes requiring a major bump are incompatible. Assume a minor
        // release so lints requiring a minor bump (e.g. adding #[must_use]) don't
        // also exit 100 and incorrectly trigger a breaking version bump.
        .args(["--release-type", "minor"])
        .arg("--manifest-path")
        .arg(local_package.join(CARGO_TOML))
        .arg("--baseline-root")
        .arg(registry_package.join(CARGO_TOML))
        .output()
        .with_context(|| format!("error while running cargo-semver-checks on {local_package:?}"))?;

    // Delete Cargo.lock file if cargo-semver-checks created it.
    if !local_package_contained_cargo_lock && local_cargo_lock.exists() {
        fs_err::remove_file(local_cargo_lock)?;
    }
    if !registry_package_contained_cargo_lock && registry_cargo_lock.exists() {
        fs_err::remove_file(registry_cargo_lock)?;
    }
    // Delete target dir if cargo-semver-checks created it.
    if !local_package_contained_target && local_target_dir.exists() {
        fs_err::remove_dir_all(local_target_dir)?;
    }
    if !registry_package_contained_target && registry_target_dir.exists() {
        fs_err::remove_dir_all(registry_target_dir)?;
    }

    parse_semver_check_output(&output)
        .with_context(|| format!("error while running cargo-semver-checks on {local_package:?}"))
}

fn parse_semver_check_output(output: &Output) -> anyhow::Result<SemverCheck> {
    match output.status.code() {
        Some(0) => Ok(SemverCheck::Compatible),
        // With --release-type minor, exit code 100 means deny-level lint
        // violations that require a major version bump.
        Some(100) => {
            let stdout = strip_ansi_escapes::strip(&output.stdout);
            let stdout = String::from_utf8(stdout)?.trim().to_string();
            if stdout.is_empty() {
                anyhow::bail!("unknown source of semver incompatibility");
            }
            Ok(SemverCheck::Incompatible(stdout))
        }
        _ => {
            let stdout = strip_ansi_escapes::strip(&output.stdout);
            let stderr = strip_ansi_escapes::strip(&output.stderr);
            anyhow::bail!(
                "cargo-semver-checks failed with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&stdout).trim(),
                String::from_utf8_lossy(&stderr).trim(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_major_changes_are_incompatible() {
        let temp = crate::fs_utils::Utf8TempDir::new().unwrap();
        let baseline = temp.path().join("baseline");
        let current = temp.path().join("current");
        for package in [&baseline, &current] {
            fs_err::create_dir_all(package.join("src")).unwrap();
            fs_err::write(
                package.join(CARGO_TOML),
                "[package]\nname = \"semver-check-test\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
            )
            .unwrap();
        }
        fs_err::write(baseline.join("src/lib.rs"), "pub fn answer() -> u32 { 42 }").unwrap();

        let assert_incompatible = |source: &str, incompatible| {
            fs_err::write(current.join("src/lib.rs"), source).unwrap();
            let result = run_semver_check(&current, &baseline).unwrap();
            assert_eq!(
                matches!(result, SemverCheck::Incompatible(_)),
                incompatible,
                "unexpected result for {source}: {result:?}",
            );
        };

        // Adding #[must_use] requires only a minor bump in cargo-semver-checks.
        assert_incompatible("#[must_use]\npub fn answer() -> u32 { 42 }", false);
        // Removing the existing public function requires a major bump.
        assert_incompatible("pub fn other() -> u32 { 42 }", true);
    }
}
