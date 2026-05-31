use std::process::Command;

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

    let stderr = String::from_utf8(output.stderr)?;
    let stdout = output.stdout;
    classify_semver_check_output(output.status.success(), &stderr, &stdout)
}

/// Decide what `cargo-semver-checks` told us based on its exit status and output.
///
/// cargo-semver-checks uses exit code 1 for both detected breaking changes and
/// tool failures (e.g. unsupported rustdoc format, IO errors), so we have to
/// look at stderr to tell them apart. Without that distinction a tool failure
/// silently gets reported as `(✓ API compatible changes)` and the user
/// publishes a wrong version.
/// See <https://github.com/release-plz/release-plz/issues/2294>
fn classify_semver_check_output(
    success: bool,
    stderr: &str,
    stdout: &[u8],
) -> anyhow::Result<SemverCheck> {
    if success {
        return Ok(SemverCheck::Compatible);
    }

    if stderr.contains("semver requires new major version") {
        let stdout = strip_ansi_escapes::strip(stdout);
        let stdout = String::from_utf8(stdout)?.trim().to_string();
        if stdout.is_empty() {
            anyhow::bail!("unknown source of semver incompatibility");
        }
        return Ok(SemverCheck::Incompatible(stdout));
    }

    // Some failures are not "the tool is broken" but "the tool can't analyze
    // this package" (e.g. when the registry baseline package legitimately has
    // no rustdoc-able lib target — exercised by the integration suite when a
    // package was a binary in the previous release). These are not actionable
    // bugs to surface; preserve the historical behavior of treating them as
    // compatible. Everything else is a real tool failure.
    if stderr.contains("no library targets found") {
        return Ok(SemverCheck::Compatible);
    }

    // Non-zero exit without the breaking-change marker and without a known
    // "tool can't analyze" signal indicates cargo-semver-checks itself
    // failed (e.g. `unsupported rustdoc format`). Surface it to the user
    // instead of silently claiming the change is API-compatible.
    anyhow::bail!(
        "cargo-semver-checks failed (the change may or may not be API compatible — release-plz cannot tell). stderr: {}",
        stderr.trim()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_compatible() {
        let outcome = classify_semver_check_output(true, "", b"").unwrap();
        assert!(matches!(outcome, SemverCheck::Compatible));
    }

    /// Exit 1 with the well-known marker reports incompatibility, and the stdout
    /// describing the violation is propagated to the user.
    #[test]
    fn breaking_change_is_incompatible() {
        let stderr =
            "error: semver requires new major version: 1 major and 0 minor changes detected\n";
        let stdout = b"--- failure constructible_struct: ...\n";
        let outcome = classify_semver_check_output(false, stderr, stdout).unwrap();
        match outcome {
            SemverCheck::Incompatible(msg) => assert!(msg.contains("constructible_struct")),
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    /// Regression test for <https://github.com/release-plz/release-plz/issues/2294>
    ///
    /// When cargo-semver-checks itself fails (e.g. unsupported rustdoc format),
    /// it exits non-zero but does NOT print "semver requires new major version".
    /// The previous code silently treated this as "Compatible", causing users to
    /// publish minor bumps for actually-breaking changes. We must surface the
    /// failure as an error instead.
    #[test]
    fn tool_failure_is_reported_not_swallowed() {
        let stderr = "error: unsupported rustdoc format v43 for file: target/.../doc/foo.json\n";
        let result = classify_semver_check_output(false, stderr, b"");
        let err = result.expect_err(
            "tool failure must be reported as error, not silently treated as Compatible",
        );
        let msg = format!("{err:?}");
        assert!(
            msg.contains("cargo-semver-checks failed"),
            "error should clearly attribute the failure to cargo-semver-checks; got: {msg}"
        );
        assert!(
            msg.contains("rustdoc"),
            "error should preserve the underlying stderr; got: {msg}"
        );
    }

    /// When cargo-semver-checks bails because the package being analyzed has no
    /// lib target (e.g. the registry baseline of a binary→library conversion,
    /// exercised by the docker integration suite), that's not a tool bug — the
    /// check is simply not applicable. Treat it as Compatible to preserve the
    /// historical behavior for that path.
    #[test]
    fn no_library_targets_is_treated_as_compatible() {
        let stderr = "warning: placeholder v0.0.0 ignoring invalid dependency `api` which is missing a lib target\nerror: no library targets found in package `api`\nerror: failed to build rustdoc for crate api v0.1.0\n";
        let outcome = classify_semver_check_output(false, stderr, b"").unwrap();
        assert!(
            matches!(outcome, SemverCheck::Compatible),
            "no-library-targets should not be treated as a tool failure; got {outcome:?}"
        );
    }
}
