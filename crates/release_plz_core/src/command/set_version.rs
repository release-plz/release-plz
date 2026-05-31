use std::collections::BTreeMap;

use anyhow::Context;
use cargo_metadata::{
    Metadata, Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_utils::{LocalManifest, canonical_local_manifest, workspace_members};

use crate::{CHANGELOG_FILENAME, PackagePath as _, changelog_parser::last_release_from_str};

#[derive(Debug)]
pub struct SetVersionRequest {
    /// The manifest of the project you want to update.
    manifest: Utf8PathBuf,
    /// Cargo metadata.
    metadata: Metadata,
    version_changes: SetVersionSpec,
}

impl SetVersionRequest {
    pub fn set_changelog_path(&mut self, package: &str, changelog_path: Utf8PathBuf) {
        match &mut self.version_changes {
            SetVersionSpec::Single(change) => {
                change.changelog_path = Some(changelog_path);
            }
            SetVersionSpec::Workspace(changes) => {
                changes.entry(package.to_string()).and_modify(|change| {
                    change.with_changelog_path(changelog_path);
                });
            }
        }
    }

    /// Apply a workspace-level default changelog path to every package that
    /// does not already have its own per-package override.
    ///
    /// Without this, `[workspace] changelog_path = "./CHANGELOG.md"` would be
    /// silently ignored by `set-version` and the command would look for a
    /// per-crate `CHANGELOG.md` that doesn't exist in monorepos that share a
    /// single workspace changelog.
    /// See <https://github.com/release-plz/release-plz/issues/2441>
    pub fn set_default_changelog_path(&mut self, changelog_path: Utf8PathBuf) {
        match &mut self.version_changes {
            SetVersionSpec::Single(change) => {
                if change.changelog_path.is_none() {
                    change.changelog_path = Some(changelog_path);
                }
            }
            SetVersionSpec::Workspace(changes) => {
                for change in changes.values_mut() {
                    if change.changelog_path.is_none() {
                        change.changelog_path = Some(changelog_path.clone());
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum SetVersionSpec {
    /// Used for projects with a single package.
    /// In this case there's no need to specify the package name.
    Single(VersionChange),
    /// <package name, version change>
    /// Used for multiple packages in a workspace.
    Workspace(BTreeMap<String, VersionChange>),
}

#[derive(Debug)]
pub struct VersionChange {
    version: Version,
    /// This path needs to be a relative path to the Cargo.toml of the project.
    /// I.e. if you have a workspace, it needs to be relative to the workspace root.
    pub changelog_path: Option<Utf8PathBuf>,
}

impl VersionChange {
    pub fn new(version: Version) -> Self {
        Self {
            version,
            changelog_path: None,
        }
    }

    pub fn with_changelog_path(&mut self, changelog_path: Utf8PathBuf) {
        self.changelog_path = Some(changelog_path);
    }
}

impl SetVersionRequest {
    pub fn new(version_changes: SetVersionSpec, metadata: Metadata) -> anyhow::Result<Self> {
        let manifest = cargo_utils::workspace_manifest(&metadata);
        let manifest = canonical_local_manifest(manifest.as_ref())?;
        Ok(Self {
            version_changes,
            metadata,
            manifest,
        })
    }
}

pub fn set_version(input: &SetVersionRequest) -> anyhow::Result<()> {
    let workspace_manifest = LocalManifest::try_new(&input.manifest)?;
    let packages: BTreeMap<String, Package> = workspace_members(&input.metadata)?
        .map(|p| {
            let package_name = p.name.to_string();
            (package_name, p)
        })
        .collect();
    let all_packages: Vec<&Package> = packages.values().collect();
    match &input.version_changes {
        SetVersionSpec::Single(change) => {
            anyhow::ensure!(
                packages.len() == 1,
                "Your workspace contains multiple packages. Please specify which package you want to update."
            );
            let package = packages.keys().next().unwrap();
            set_version_in_package(
                &packages,
                package,
                &all_packages,
                change,
                &workspace_manifest,
            )?;
        }
        SetVersionSpec::Workspace(changes) => {
            for (package, change) in changes {
                set_version_in_package(
                    &packages,
                    package,
                    &all_packages,
                    change,
                    &workspace_manifest,
                )?;
            }
        }
    }
    Ok(())
}

fn set_version_in_package(
    packages: &BTreeMap<String, Package>,
    package: &String,
    all_packages: &[&Package],
    change: &VersionChange,
    workspace_manifest: &LocalManifest,
) -> Result<(), anyhow::Error> {
    let pkg = packages
        .get(package)
        .with_context(|| format!("package {package} not found"))?;
    let pkg_path = pkg.package_path()?;
    super::update::set_version(
        all_packages,
        pkg_path,
        &change.version,
        &workspace_manifest.path,
    )?;
    let default_changelog_path = pkg_path.join(CHANGELOG_FILENAME);
    // Resolve a configured `changelog_path` relative to the workspace
    // manifest's parent (matching how `update` interprets it). Without this,
    // a value like `./CHANGELOG.md` would be resolved against the process
    // cwd, which is unreliable and inconsistent with the rest of release-plz.
    let resolved_changelog_path: Option<Utf8PathBuf> = change.changelog_path.as_ref().map(|p| {
        if p.is_absolute() {
            p.clone()
        } else if let Some(workspace_dir) = workspace_manifest.path.parent() {
            workspace_dir.join(p)
        } else {
            p.clone()
        }
    });
    let changelog_path: &Utf8Path = resolved_changelog_path
        .as_deref()
        .unwrap_or(&default_changelog_path);
    update_changelog(changelog_path, &pkg.version, &change.version)
        .with_context(|| format!("failed to update changelog at {changelog_path}"))?;
    Ok(())
}

fn update_changelog(
    changelog_path: &Utf8Path,
    old_version: &Version,
    new_version: &Version,
) -> anyhow::Result<()> {
    let changelog_content = fs_err::read_to_string(changelog_path)?;
    let last_release = last_release_from_str(&changelog_content)?.context("no release found")?;

    let new_changelog_content = {
        let old_title = last_release.title();
        // replace the new version. `replacen` doesn't work, because we
        // also want to replace the version in the release link.
        let new_title = old_title.replace(&old_version.to_string(), &new_version.to_string());
        changelog_content.replacen(old_title, &new_title, 1)
    };

    fs_err::write(changelog_path, new_changelog_content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_change() -> VersionChange {
        VersionChange::new(Version::new(1, 0, 0))
    }

    /// Workspace-default `changelog_path` should land on every package that
    /// doesn't have its own override. Regression for
    /// <https://github.com/release-plz/release-plz/issues/2441>.
    #[test]
    fn workspace_default_propagates_to_all_packages() {
        let mut spec = SetVersionSpec::Workspace(BTreeMap::from([
            ("a".to_string(), dummy_change()),
            ("b".to_string(), dummy_change()),
        ]));
        let mut req = SetVersionRequest {
            manifest: Utf8PathBuf::from("/tmp/Cargo.toml"),
            metadata: empty_metadata(),
            version_changes: spec_take(&mut spec),
        };
        req.set_default_changelog_path(Utf8PathBuf::from("./CHANGELOG.md"));
        let SetVersionSpec::Workspace(changes) = req.version_changes else {
            unreachable!()
        };
        assert_eq!(
            changes["a"].changelog_path.as_deref(),
            Some(Utf8Path::new("./CHANGELOG.md")),
        );
        assert_eq!(
            changes["b"].changelog_path.as_deref(),
            Some(Utf8Path::new("./CHANGELOG.md")),
        );
    }

    /// A per-package override applied first must NOT be replaced by a
    /// subsequent workspace default.
    #[test]
    fn per_package_override_wins_over_workspace_default() {
        let mut changes = BTreeMap::new();
        let mut a = dummy_change();
        a.with_changelog_path(Utf8PathBuf::from("./crates/a/CHANGELOG.md"));
        changes.insert("a".to_string(), a);
        changes.insert("b".to_string(), dummy_change());
        let mut spec = SetVersionSpec::Workspace(changes);
        let mut req = SetVersionRequest {
            manifest: Utf8PathBuf::from("/tmp/Cargo.toml"),
            metadata: empty_metadata(),
            version_changes: spec_take(&mut spec),
        };
        req.set_default_changelog_path(Utf8PathBuf::from("./CHANGELOG.md"));
        let SetVersionSpec::Workspace(changes) = req.version_changes else {
            unreachable!()
        };
        assert_eq!(
            changes["a"].changelog_path.as_deref(),
            Some(Utf8Path::new("./crates/a/CHANGELOG.md")),
            "package-specific changelog_path must not be clobbered by the workspace default"
        );
        assert_eq!(
            changes["b"].changelog_path.as_deref(),
            Some(Utf8Path::new("./CHANGELOG.md")),
        );
    }

    #[test]
    fn workspace_default_applies_to_single_spec() {
        let mut req = SetVersionRequest {
            manifest: Utf8PathBuf::from("/tmp/Cargo.toml"),
            metadata: empty_metadata(),
            version_changes: SetVersionSpec::Single(dummy_change()),
        };
        req.set_default_changelog_path(Utf8PathBuf::from("./CHANGELOG.md"));
        let SetVersionSpec::Single(change) = req.version_changes else {
            unreachable!()
        };
        assert_eq!(
            change.changelog_path.as_deref(),
            Some(Utf8Path::new("./CHANGELOG.md")),
        );
    }

    fn spec_take(spec: &mut SetVersionSpec) -> SetVersionSpec {
        let placeholder = SetVersionSpec::Workspace(BTreeMap::new());
        std::mem::replace(spec, placeholder)
    }

    fn empty_metadata() -> Metadata {
        // Minimal valid cargo metadata JSON. The set_default_changelog_path
        // tests don't touch metadata, but Rust insists we construct one.
        let json = r#"{
            "packages": [],
            "workspace_members": [],
            "workspace_default_members": [],
            "resolve": null,
            "target_directory": "/tmp/target",
            "version": 1,
            "workspace_root": "/tmp",
            "metadata": null
        }"#;
        serde_json::from_str(json).expect("valid empty metadata JSON")
    }
}
