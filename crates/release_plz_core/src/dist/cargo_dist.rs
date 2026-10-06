use std::{
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context as _, ensure};
use cargo_metadata::{Metadata, Package, camino::Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use serde::Serialize;
use toml_edit::{DocumentMut, value};

use super::Manifest;
use crate::{Project, fs_utils, tmp_repo::TempRepo};

pub const VERSION: &str = "0.33.0";
/// The cargo-dist executable name. `Command` resolves `dist.exe` on Windows.
const EXECUTABLE: &str = "dist";

#[derive(Debug)]
pub(super) struct CargoDist {
    _repo: TempRepo,
    root: Utf8PathBuf,
    tag: String,
    targets: Vec<String>,
}

/// The `dist-workspace.toml` written to the temporary workspace.
#[derive(Serialize)]
struct DistWorkspace<'a> {
    workspace: WorkspaceConfig,
    dist: DistConfig<'a>,
}

#[derive(Serialize)]
struct WorkspaceConfig {
    members: &'static [&'static str],
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct DistConfig<'a> {
    cargo_dist_version: &'static str,
    /// release-plz drives CI itself: cargo-dist must not generate workflows.
    ci: &'static [&'static str],
    hosting: &'static [&'static str],
    installers: &'static [&'static str],
    targets: &'a [String],
    source_tarball: bool,
    precise_builds: bool,
}

impl CargoDist {
    pub fn prepare(
        project: &Project,
        metadata: &Metadata,
        package: &Package,
        repository: &str,
        tag: &str,
        targets: Vec<String>,
    ) -> anyhow::Result<Self> {
        check_executable(Path::new(EXECUTABLE))?;
        let original_root = project.root();
        let repo = project.get_repo()?;
        let root = repo.repo.directory().join(fs_utils::strip_prefix(
            &metadata.workspace_root,
            original_root,
        )?);
        ensure!(
            !root.join("dist-workspace.toml").exists(),
            "distribute=true manages its own cargo-dist configuration; remove dist-workspace.toml or use cargo-dist independently"
        );
        for member in metadata.workspace_packages() {
            let path = repo.repo.directory().join(fs_utils::strip_prefix(
                &member.manifest_path,
                original_root,
            )?);
            let mut manifest: DocumentMut = fs_err::read_to_string(&path)?.parse()?;
            ensure!(
                manifest
                    .get("package")
                    .and_then(|p| p.get("metadata"))
                    .and_then(|m| m.get("dist"))
                    .is_none(),
                "distribute=true cannot be combined with package.metadata.dist ({})",
                member.name
            );
            manifest["package"]["metadata"]["dist"]["dist"] = value(member.id == package.id);
            // Ensure installers always link to the repository that owns this release,
            // including private, git-only packages and inherited repository fields.
            manifest["package"]["repository"] = value(repository);
            fs_err::write(path, manifest.to_string())?;
        }
        prepare_root_manifest(root.join(CARGO_TOML).as_std_path())?;
        let config = toml::to_string(&DistWorkspace {
            workspace: WorkspaceConfig {
                members: &["cargo:."],
            },
            dist: DistConfig {
                cargo_dist_version: VERSION,
                ci: &[],
                hosting: &["github"],
                installers: &["shell", "powershell"],
                targets: &targets,
                source_tarball: false,
                precise_builds: true,
            },
        })?;
        fs_err::write(root.join("dist-workspace.toml"), config)?;
        Ok(Self {
            _repo: repo,
            root,
            tag: tag.to_owned(),
            targets,
        })
    }

    /// Run `dist build` for the prepared targets: local archives, or the global installers.
    pub fn build(&self, global: bool) -> anyhow::Result<Manifest> {
        let mut cmd = Command::new(EXECUTABLE);
        cmd.current_dir(&self.root)
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .args([
                "build",
                "--output-format=json",
                "--allow-dirty",
                "--tag",
                &self.tag,
                if global {
                    "--artifacts=global"
                } else {
                    "--artifacts=local"
                },
            ])
            .stderr(Stdio::inherit());
        for target in &self.targets {
            cmd.args(["--target", target]);
        }
        let output = cmd.output().context("cannot run cargo-dist")?;
        ensure!(
            output.status.success(),
            "cargo-dist failed; the GitHub release remains a draft"
        );
        serde_json::from_slice(&output.stdout).context("invalid cargo-dist manifest")
    }

    pub fn import_manifest(&self, index: usize, manifest: &Manifest) -> anyhow::Result<()> {
        let dir = self.root.join("target/distrib");
        fs_err::create_dir_all(&dir)?;
        fs_err::write(
            dir.join(format!("{index}-dist-manifest.json")),
            serde_json::to_vec(manifest)?,
        )?;
        Ok(())
    }

    pub fn artifact_bytes(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        let path = fs_err::canonicalize(path)?;
        ensure!(
            path.starts_with(fs_err::canonicalize(&self.root)?),
            "cargo-dist artifact is outside the temporary workspace"
        );
        fs_err::read(path).context("cannot read cargo-dist artifact")
    }
}

/// Reject a workspace-level cargo-dist table and define the `dist` profile if it is missing.
fn prepare_root_manifest(manifest_path: &Path) -> anyhow::Result<()> {
    let mut manifest: DocumentMut = fs_err::read_to_string(manifest_path)?.parse()?;
    ensure!(
        manifest
            .get("workspace")
            .and_then(|w| w.get("metadata"))
            .and_then(|m| m.get("dist"))
            .is_none(),
        "distribute=true cannot be combined with workspace.metadata.dist; remove it or use cargo-dist independently"
    );
    if manifest
        .get("profile")
        .and_then(|p| p.get("dist"))
        .is_none()
    {
        // cargo-dist always builds with --profile dist. Define the fallback only in
        // the temporary manifest so Cargo config and environment overrides still win.
        manifest["profile"]["dist"]["inherits"] = value("release");
        fs_err::write(manifest_path, manifest.to_string())?;
    }
    Ok(())
}

fn check_executable(executable: &Path) -> anyhow::Result<()> {
    let output = Command::new(executable).arg("--version").output()
        .with_context(|| format!("cargo-dist {VERSION} is required; install it and make its `dist` executable available on PATH"))?;
    ensure!(
        output.status.success(),
        "failed to check cargo-dist version: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    ensure_version(&String::from_utf8_lossy(&output.stdout))
}

/// Check the output of `dist --version` against the pinned version.
fn ensure_version(stdout: &str) -> anyhow::Result<()> {
    ensure!(
        stdout.split_whitespace().last() == Some(VERSION),
        "cargo-dist {VERSION} is required; found `{}`",
        stdout.trim()
    );
    Ok(())
}

pub(super) fn host_target() -> anyhow::Result<String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .context("cannot run rustc")?;
    ensure!(output.status.success(), "rustc -vV failed");
    String::from_utf8(output.stdout)?
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .context("rustc did not report a host target")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_executable_reports_the_required_installation() {
        let temporary = tempfile::tempdir().unwrap();
        let error = check_executable(&temporary.path().join("missing-dist")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("cargo-dist {VERSION} is required"))
        );
        assert!(error.to_string().contains("PATH"));
        assert!(fs_err::read_dir(temporary.path()).unwrap().next().is_none());
    }

    #[test]
    fn version_mismatch_reports_required_and_found_versions() {
        ensure_version("cargo-dist 0.33.0\n").unwrap();
        let error = ensure_version("cargo-dist 0.32.0\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("cargo-dist 0.33.0 is required"), "{error}");
        assert!(error.contains("found `cargo-dist 0.32.0`"), "{error}");
    }

    #[test]
    fn missing_dist_profile_inherits_release_without_extra_settings() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join(CARGO_TOML);
        for manifest in ["[workspace]\n", "[profile.release]\nlto = true\n"] {
            fs_err::write(&path, manifest).unwrap();
            prepare_root_manifest(&path).unwrap();
            let actual: toml::Value =
                toml::from_str(&fs_err::read_to_string(&path).unwrap()).unwrap();
            let expected: toml::Value = toml::from_str(&format!(
                "{manifest}\n[profile.dist]\ninherits = 'release'\n"
            ))
            .unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn existing_dist_profile_is_preserved() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join(CARGO_TOML);
        let manifest = "[profile.dist]\ninherits = 'release'\nlto = false\ncodegen-units = 4\n";
        fs_err::write(&path, manifest).unwrap();
        prepare_root_manifest(&path).unwrap();
        assert_eq!(fs_err::read_to_string(&path).unwrap(), manifest);
    }

    #[test]
    fn workspace_dist_metadata_is_rejected_before_rewriting_the_manifest() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join(CARGO_TOML);
        let manifest = "[workspace]\nmembers = ['app']\n\n[workspace.metadata.dist]\ndist = true\n";
        fs_err::write(&path, manifest).unwrap();
        let error = prepare_root_manifest(&path).unwrap_err().to_string();
        assert!(
            error.contains("cannot be combined with workspace.metadata.dist"),
            "{error}"
        );
        assert_eq!(fs_err::read_to_string(&path).unwrap(), manifest);
    }
}
