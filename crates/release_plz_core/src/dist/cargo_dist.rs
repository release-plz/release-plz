use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context as _, ensure};
use cargo_metadata::camino::Utf8PathBuf;
use cargo_metadata::{Metadata, Package};
use toml_edit::{DocumentMut, value};

use super::Manifest;
use crate::{Project, root_repo_path_from_manifest_dir, tmp_repo::TempRepo};

pub const VERSION: &str = "0.33.0";

#[derive(Debug)]
pub(super) struct CargoDist {
    _repo: TempRepo,
    pub root: Utf8PathBuf,
    executable: PathBuf,
}

impl CargoDist {
    pub fn prepare(
        project: &Project,
        metadata: &Metadata,
        package: &Package,
        repository: &str,
        targets: &[String],
    ) -> anyhow::Result<Self> {
        check_dist_profile(metadata.workspace_root.join("Cargo.toml").as_std_path())?;
        let executable = PathBuf::from(format!("dist{}", std::env::consts::EXE_SUFFIX));
        check_executable(&executable)?;
        let original_root = root_repo_path_from_manifest_dir(&metadata.workspace_root)?;
        let repo = project.get_repo()?;
        let root = repo
            .repo
            .directory()
            .join(metadata.workspace_root.strip_prefix(&original_root)?);
        ensure!(
            !root.join("dist-workspace.toml").exists(),
            "dist=true manages its own cargo-dist configuration; remove dist-workspace.toml or use cargo-dist independently"
        );
        for member in &metadata.packages {
            if !metadata.workspace_members.contains(&member.id) {
                continue;
            }
            let path = repo
                .repo
                .directory()
                .join(member.manifest_path.strip_prefix(&original_root)?);
            let mut manifest: DocumentMut = fs_err::read_to_string(&path)?.parse()?;
            ensure!(
                manifest
                    .get("package")
                    .and_then(|p| p.get("metadata"))
                    .and_then(|m| m.get("dist"))
                    .is_none(),
                "dist=true cannot be combined with package.metadata.dist ({})",
                member.name
            );
            manifest["package"]["metadata"]["dist"]["dist"] = value(member.id == package.id);
            // Ensure installers always link to the repository that owns this release,
            // including private, git-only packages and inherited repository fields.
            manifest["package"]["repository"] = value(repository);
            fs_err::write(path, manifest.to_string())?;
        }
        let config = toml::to_string(&serde_json::json!({
            "workspace": {"members": ["cargo:."]},
            "dist": {
                "cargo-dist-version": VERSION, "ci": [], "hosting": ["github"],
                "installers": ["shell", "powershell"], "targets": targets,
                "source-tarball": false, "precise-builds": true
            }
        }))?;
        fs_err::write(root.join("dist-workspace.toml"), config)?;
        Ok(Self {
            _repo: repo,
            root,
            executable,
        })
    }

    pub fn build(&self, tag: &str, targets: &[String], global: bool) -> anyhow::Result<Manifest> {
        let mut cmd = Command::new(&self.executable);
        cmd.current_dir(&self.root)
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .args([
                "build",
                "--output-format=json",
                "--allow-dirty",
                "--tag",
                tag,
                if global {
                    "--artifacts=global"
                } else {
                    "--artifacts=local"
                },
            ])
            .stderr(Stdio::inherit());
        for target in targets {
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

fn check_dist_profile(manifest_path: &Path) -> anyhow::Result<()> {
    let manifest: DocumentMut = fs_err::read_to_string(manifest_path)?.parse()?;
    ensure!(
        manifest
            .get("profile")
            .and_then(|p| p.get("dist"))
            .is_some_and(|p| p.is_table_like()),
        "missing [profile.dist] in {}; add it with inherits = \"release\" (suggested: lto = \"thin\")",
        manifest_path.display()
    );
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
    let version = String::from_utf8_lossy(&output.stdout);
    ensure!(
        version.split_whitespace().last() == Some(VERSION),
        "cargo-dist {VERSION} is required; found `{}`",
        version.trim()
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
    fn dist_profile_is_required_and_never_written() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("Cargo.toml");
        for manifest in ["[workspace]\n", "[profile.release]\nlto = true\n"] {
            fs_err::write(&path, manifest).unwrap();
            let error = check_dist_profile(&path).unwrap_err();
            assert!(error.to_string().contains("missing [profile.dist]"));
            assert_eq!(fs_err::read_to_string(&path).unwrap(), manifest);
        }
        let manifest = "[profile.dist]\ninherits = 'release'\nlto = false\ncodegen-units = 4\n";
        fs_err::write(&path, manifest).unwrap();
        check_dist_profile(&path).unwrap();
        assert_eq!(fs_err::read_to_string(&path).unwrap(), manifest);
    }
}
