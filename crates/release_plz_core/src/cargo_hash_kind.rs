use anyhow::Context as _;
use cargo_metadata::semver::Version;
use tracing::warn;

use crate::{cargo::run_cargo, fs_utils};

fn cargo_version_from_stdout(stdout: &str) -> anyhow::Result<Version> {
    let version = stdout
        .split_whitespace()
        .nth(1)
        .with_context(|| format!("failed to parse cargo version from cargo stdout `{stdout}`"))?;
    let version = Version::parse(version)
        .with_context(|| format!("failed to parse cargo version from version `{version}`"))?;
    Ok(version)
}

fn cargo_version_stdout() -> anyhow::Result<String> {
    let current_directory = fs_utils::current_directory()?;
    let output = run_cargo(&current_directory, &["--version"])
        .context("failed to run cargo --version. Is cargo installed?")?;
    Ok(output.stdout)
}

pub fn get_hash_kind() -> anyhow::Result<crates_index::HashKind> {
    let output = cargo_version_stdout()?;
    let hash_kind = get_hash_kind_from_stdout(&output);
    Ok(hash_kind)
}

fn get_hash_kind_from_stdout(output: &str) -> crates_index::HashKind {
    match cargo_version_from_stdout(output) {
        Ok(version) => {
            if version < Version::new(1, 85, 0) {
                crates_index::HashKind::Legacy
            } else {
                // With edition 2024 (cargo 1.85.0) the hash kind changed.
                // See https://doc.rust-lang.org/nightly/cargo/CHANGELOG.html
                crates_index::HashKind::Stable
            }
        }
        Err(e) => {
            warn!(
                "Error parsing cargo version: {:?}. Assuming cargo > 1.85.0",
                e
            );
            crates_index::HashKind::Stable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_version_stable() {
        let stdout = "cargo 1.85.0 (d73d2caf9 2024-12-31)";
        let version = cargo_version_from_stdout(stdout).unwrap();
        assert!(version == Version::new(1, 85, 0));
        let hash_kind = get_hash_kind_from_stdout(stdout);
        assert!(matches!(hash_kind, crates_index::HashKind::Stable));
    }

    #[test]
    fn test_cargo_version_old_stable() {
        let stdout = "cargo 1.84.0 (abc12345 2024-01-01)";
        let version = cargo_version_from_stdout(stdout).unwrap();
        assert!(version == Version::new(1, 84, 0));
        let hash_kind = get_hash_kind_from_stdout(stdout);
        assert!(matches!(hash_kind, crates_index::HashKind::Legacy));
    }

    #[test]
    fn test_cargo_version_nightly() {
        let stdout = "cargo 1.87.0-nightly (ce948f461 2025-02-14)";
        let version = cargo_version_from_stdout(stdout).unwrap();
        assert!(version >= Version::new(1, 85, 0));
        let hash_kind = get_hash_kind_from_stdout(stdout);
        assert!(matches!(hash_kind, crates_index::HashKind::Stable));
    }
}
