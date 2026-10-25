use std::process::{Command, Output};

use anyhow::Context;
use cargo_metadata::{
    Metadata,
    camino::{Utf8Path, Utf8PathBuf},
};
use cargo_utils::CARGO_TOML;

use crate::fs_utils::Utf8TempDir;

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

/// Where cargo-semver-checks builds the rustdoc JSON of the compared versions.
#[derive(Debug, Clone, Copy)]
pub enum TargetDir {
    /// Inherit the target directory set by the user, to reuse their build cache.
    Configured,
    /// Use a temporary target directory per package, so the baseline reuses the
    /// dependencies built for the current version. Packages don't share it because
    /// cargo runs one build at a time per target directory.
    PerPackage,
}

impl TargetDir {
    pub fn new(metadata: &Metadata) -> Self {
        // `cargo metadata` already reflects these variables, but they can explicitly
        // point to the default `target` directory (e.g. to cache it in CI): keep that cache.
        if std::env::var_os("CARGO_TARGET_DIR").is_none()
            && std::env::var_os("CARGO_BUILD_TARGET_DIR").is_none()
            && metadata.target_directory == metadata.workspace_root.join("target")
        {
            Self::PerPackage
        } else {
            Self::Configured
        }
    }
}

pub fn run_semver_check(
    local_package: &Utf8Path,
    registry_package: &Utf8Path,
    target_dir: TargetDir,
) -> anyhow::Result<SemverCheck> {
    let local_manifest = local_package.join(CARGO_TOML);
    let manifest: cargo_utils::Manifest = fs_err::read_to_string(&local_manifest)?.parse()?;
    let package_name = manifest
        .data
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml_edit::Item::as_str)
        .context("cannot find the package name to check for API compatibility")?;
    let local_cargo_lock = cargo_lock(local_package);
    let registry_cargo_lock = cargo_lock(registry_package);

    let local_package_contained_cargo_lock = local_cargo_lock.exists();
    let registry_package_contained_cargo_lock = registry_cargo_lock.exists();

    let mut command = Command::new("cargo-semver-checks");
    // Kept alive until the check is over: dropping it deletes the directory.
    let _package_target_dir = match target_dir {
        TargetDir::Configured => None,
        TargetDir::PerPackage => {
            let dir = Utf8TempDir::new()?;
            command.env("CARGO_TARGET_DIR", dir.path());
            Some(dir)
        }
    };
    let output = command
        .args(["semver-checks", "check-release"])
        .args(["--color", "never"])
        // Only changes requiring a major bump are incompatible. Assume a minor
        // release so lints requiring a minor bump (e.g. adding #[must_use]) don't
        // also exit 100 and incorrectly trigger a breaking version bump.
        .args(["--release-type", "minor"])
        .arg("--manifest-path")
        .arg(local_manifest)
        // Explicitly select private workspace members, which cargo-semver-checks
        // otherwise skips even when their manifest path is provided.
        .args(["--package", package_name])
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

    parse_semver_check_output(&output)
        .with_context(|| format!("error while running cargo-semver-checks on {local_package:?}"))
}

fn parse_semver_check_output(output: &Output) -> anyhow::Result<SemverCheck> {
    match output.status.code() {
        Some(0) => Ok(SemverCheck::Compatible),
        // With --release-type minor, exit code 100 means deny-level lint
        // violations that require a major version bump.
        Some(100) => {
            let stdout = std::str::from_utf8(&output.stdout)?.trim().to_string();
            if stdout.is_empty() {
                anyhow::bail!("unknown source of semver incompatibility");
            }
            Ok(SemverCheck::Incompatible(stdout))
        }
        _ => {
            anyhow::bail!(
                "cargo-semver-checks failed with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_private_library_with_versionless_dependency() {
        let temp = Utf8TempDir::new().unwrap();
        let baseline = temp.path().join("baseline");
        let current = temp.path().join("current");
        for workspace in [&baseline, &current] {
            crate::test_utils::write_package(
                &workspace.join("support"),
                "support",
                "0.1.0",
                "publish = false\n",
            );
            fs_err::write(
                workspace.join("support/src/lib.rs"),
                "pub fn answer() -> u32 { 42 }\n",
            )
            .unwrap();
            crate::test_utils::write_package(
                &workspace.join("wrapper"),
                "wrapper",
                "0.1.0",
                "publish = false\n[dependencies]\nsupport = { path = \"../support\" }\n",
            );
            fs_err::write(
                workspace.join("wrapper/src/lib.rs"),
                "pub fn answer() -> u32 { support::answer() }\npub fn removed() {}\n",
            )
            .unwrap();
            fs_err::write(
                workspace.join("Cargo.toml"),
                "[workspace]\nmembers = [\"support\", \"wrapper\"]\nresolver = \"3\"\n",
            )
            .unwrap();
        }

        let baseline = baseline.join("wrapper");
        let current = current.join("wrapper");
        assert!(matches!(
            run_semver_check(&current, &baseline, TargetDir::PerPackage).unwrap(),
            SemverCheck::Compatible
        ));

        fs_err::write(
            current.join("src/lib.rs"),
            "pub fn answer() -> u32 { support::answer() }\n",
        )
        .unwrap();
        let result = run_semver_check(&current, &baseline, TargetDir::PerPackage).unwrap();
        assert!(
            matches!(result, SemverCheck::Incompatible(ref report) if report.contains("function_missing")),
            "expected a removed-function report for the private library, got {result:?}"
        );
    }

    #[test]
    fn only_major_changes_are_incompatible() {
        let temp = Utf8TempDir::new().unwrap();
        let baseline = temp.path().join("baseline");
        let current = temp.path().join("current");
        for package in [&baseline, &current] {
            crate::test_utils::write_package(package, "semver-check-test", "1.0.0", "");
        }
        fs_err::write(baseline.join("src/lib.rs"), "pub fn answer() -> u32 { 42 }").unwrap();

        let assert_incompatible = |source: &str, incompatible| {
            fs_err::write(current.join("src/lib.rs"), source).unwrap();
            let result = run_semver_check(&current, &baseline, TargetDir::PerPackage).unwrap();
            assert_eq!(
                matches!(result, SemverCheck::Incompatible(_)),
                incompatible,
                "unexpected result for {source}: {result:?}",
            );
        };

        // Adding #[must_use] requires only a minor bump in cargo-semver-checks.
        assert_incompatible("#[must_use]\npub fn answer() -> u32 { 42 }", false);
        // Removing the existing public function requires a major bump.
        // Both versions have the same name and version, so in their shared target dir they
        // produce the same rustdoc JSON file. This change is only detected if the baseline
        // doesn't overwrite the current rustdoc before they are compared.
        assert_incompatible("pub fn other() -> u32 { 42 }", true);
    }
}
