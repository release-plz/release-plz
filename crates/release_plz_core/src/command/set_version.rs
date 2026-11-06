use std::{
    collections::{BTreeMap, btree_map::Entry},
    ops::Range,
};

use anyhow::Context;
use cargo_metadata::{
    Metadata, Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_utils::{LocalManifest, canonical_local_manifest, workspace_members};
use tracing::warn;

use crate::{
    PackagePath as _, PackageUpdateConfig, UpdateConfig, changelog_parser,
    update_request::PackagesConfig,
};

#[derive(Debug)]
pub struct SetVersionRequest {
    /// The manifest of the project you want to update.
    manifest: Utf8PathBuf,
    /// Cargo metadata.
    metadata: Metadata,
    version_changes: SetVersionSpec,
    /// Package-specific configurations. Only `changelog_path` and `changelog_update` are used.
    packages_config: PackagesConfig,
}

impl SetVersionRequest {
    pub fn new(version_changes: SetVersionSpec, metadata: Metadata) -> anyhow::Result<Self> {
        let manifest = cargo_utils::workspace_manifest(&metadata);
        let manifest = canonical_local_manifest(manifest.as_ref())?;
        Ok(Self {
            version_changes,
            metadata,
            manifest,
            packages_config: PackagesConfig::default(),
        })
    }

    /// Set update config for all packages.
    pub fn with_default_package_config(mut self, config: UpdateConfig) -> Self {
        self.packages_config.set_default(config);
        self
    }

    /// Set update config for a specific package.
    pub fn with_package_config(
        mut self,
        package: impl Into<String>,
        config: PackageUpdateConfig,
    ) -> Self {
        self.packages_config.set(package.into(), config);
        self
    }
}

#[derive(Debug)]
pub enum SetVersionSpec {
    /// Set the workspace version, or the package version in a single-package project.
    /// In these cases there's no need to specify the package name.
    Single(Version),
    /// <package name, new version>
    /// Used for multiple packages in a workspace.
    Workspace(BTreeMap<String, Version>),
}

pub fn set_version(input: &SetVersionRequest) -> anyhow::Result<()> {
    let mut workspace_manifest = LocalManifest::try_new(&input.manifest)?;
    let workspace_dir = crate::manifest_dir(&workspace_manifest.path)?.to_owned();
    let cargo_lock = workspace_dir.join("Cargo.lock");
    let packages: BTreeMap<String, Package> = workspace_members(&input.metadata)?
        .map(|p| {
            let package_name = p.name.to_string();
            (package_name, p)
        })
        .collect();
    let all_packages: Vec<&Package> = packages.values().collect();
    let workspace_version = match &input.version_changes {
        SetVersionSpec::Single(version) if workspace_manifest.get_workspace_version().is_some() => {
            Some(version)
        }
        _ => None,
    };
    let updating_workspace = workspace_version.is_some();
    let changes = prepare_version_changes(&input.version_changes, &packages, updating_workspace)?;
    // Edit the changelogs before writing any file, so that an invalid changelog
    // doesn't leave the manifests updated.
    let changelogs = edit_changelogs(input, &changes)?;
    if let Some(version) = workspace_version {
        // Keep version.workspace = true in the inheriting packages.
        workspace_manifest.set_workspace_version(version);
        workspace_manifest
            .write()
            .context("can't update workspace version")?;
    }
    for (package, version) in changes {
        let package_path = package.package_path()?;
        if updating_workspace {
            super::update::update_dependencies(
                &all_packages,
                version,
                package.name.as_str(),
                package_path,
                &workspace_manifest.path,
            )?;
        } else {
            super::update::set_version(
                &all_packages,
                package_path,
                version,
                &workspace_manifest.path,
            )?;
        }
    }
    for (changelog_path, changelog) in changelogs {
        fs_err::write(changelog_path, changelog)?;
    }
    if cargo_lock.exists() {
        super::update::update_cargo_lock(&workspace_dir, false)?;
    }
    Ok(())
}

fn prepare_version_changes<'a>(
    version_changes: &'a SetVersionSpec,
    packages: &'a BTreeMap<String, Package>,
    updating_workspace: bool,
) -> anyhow::Result<Vec<(&'a Package, &'a Version)>> {
    let changes = match version_changes {
        SetVersionSpec::Single(version) if updating_workspace => {
            let mut changes = Vec::new();
            for package in packages.values() {
                let manifest = LocalManifest::try_new(&package.manifest_path)?;
                if manifest.version_is_inherited() {
                    changes.push((package, version));
                }
            }
            changes
        }
        SetVersionSpec::Single(version) => {
            anyhow::ensure!(
                packages.len() == 1,
                "Your workspace contains multiple packages and no workspace version. Please specify which package you want to update."
            );
            vec![(packages.values().next().unwrap(), version)]
        }
        SetVersionSpec::Workspace(versions) => versions
            .iter()
            .map(|(name, version)| {
                let package = packages
                    .get(name)
                    .with_context(|| format!("package {name} not found"))?;
                Ok((package, version))
            })
            .collect::<anyhow::Result<_>>()?,
    };
    Ok(changes)
}

/// Rename the last release of each package in its changelog.
/// Returns the new content of the edited changelogs.
fn edit_changelogs(
    input: &SetVersionRequest,
    changes: &[(&Package, &Version)],
) -> anyhow::Result<BTreeMap<Utf8PathBuf, String>> {
    // Several packages can share a changelog, including through different relative paths.
    let mut changelogs: BTreeMap<Utf8PathBuf, EditedChangelog> = BTreeMap::new();
    for (package, new_version) in changes {
        let config = input.packages_config.get(&package.name);
        if !config.should_update_changelog() {
            continue;
        }
        let changelog_path = crate::changelog_path(
            package,
            &input.metadata.workspace_root,
            config.generic.changelog_path.as_deref(),
        );
        if !changelog_path.exists() {
            warn!(
                "{}: changelog not found at {changelog_path}, so it isn't updated",
                package.name
            );
            continue;
        }
        let resolved_path = crate::fs_utils::canonicalize_utf8(&changelog_path)
            .with_context(|| format!("failed to resolve changelog at {changelog_path}"))?;
        let changelog = match changelogs.entry(resolved_path) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let content = fs_err::read_to_string(&changelog_path)
                    .with_context(|| format!("failed to read changelog at {changelog_path}"))?;
                entry.insert(EditedChangelog {
                    content,
                    new_titles: BTreeMap::new(),
                })
            }
        };
        changelog.rename_last_release(&changelog_path, package, new_version);
    }
    Ok(changelogs
        .into_iter()
        .filter_map(|(path, changelog)| Some((path, changelog.edited_content()?)))
        .collect())
}

/// Changelog with renamed releases, kept in memory until all changelogs are validated.
struct EditedChangelog {
    /// Content of the changelog file.
    content: String,
    /// New release titles, by the start of their range in `content`.
    new_titles: BTreeMap<usize, (Range<usize>, String)>,
}

impl EditedChangelog {
    /// Rename the last release of `package` from its current version to `new_version`.
    /// Log a warning if the last release of `package` isn't its current version.
    fn rename_last_release(
        &mut self,
        changelog_path: &Utf8Path,
        package: &Package,
        new_version: &Version,
    ) {
        let old_version = package.version.to_string();
        // Search the file content, so that releases renamed for
        // other packages don't look like releases of this package.
        let Some(release) = changelog_parser::last_package_release(&self.content, &package.name)
        else {
            warn!(
                "{}: no release found in changelog {changelog_path}, so it isn't updated",
                package.name
            );
            return;
        };
        if release.version != old_version {
            warn!(
                "{}: the last release in changelog {changelog_path} is {}, not the current version {old_version}, so the changelog isn't updated",
                package.name, release.version
            );
            return;
        }
        // Replace all occurrences, because the release link contains the version too.
        let new_title = release
            .title
            .replace(&old_version, &new_version.to_string());
        // Packages can share a release, e.g. when they inherit the workspace version.
        self.new_titles
            .insert(release.title_range.start, (release.title_range, new_title));
    }

    /// Content with the renamed releases, if any release was renamed.
    fn edited_content(self) -> Option<String> {
        if self.new_titles.is_empty() {
            return None;
        }
        let mut content = self.content;
        // Start from the end, so that the ranges of the other titles stay valid.
        for (range, title) in self.new_titles.into_values().rev() {
            content.replace_range(range, &title);
        }
        Some(content)
    }
}
