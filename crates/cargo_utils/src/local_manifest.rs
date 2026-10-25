use std::{
    ops::{Deref, DerefMut},
    path::Path,
};

use anyhow::Context;
use cargo_metadata::{
    Metadata,
    camino::{Utf8Path, Utf8PathBuf},
};
use semver::Version;

use crate::{CARGO_TOML, DepKind, Manifest, to_utf8_pathbuf};

/// A Cargo manifest that is available locally.
#[derive(Debug)]
pub struct LocalManifest {
    /// Path to the manifest
    pub path: Utf8PathBuf,
    /// Manifest contents
    pub manifest: Manifest,
}

impl Deref for LocalManifest {
    type Target = Manifest;

    fn deref(&self) -> &Manifest {
        &self.manifest
    }
}

impl DerefMut for LocalManifest {
    fn deref_mut(&mut self) -> &mut Manifest {
        &mut self.manifest
    }
}

impl LocalManifest {
    /// Construct the `LocalManifest` corresponding to the `Path` provided.
    pub fn try_new(path: &Utf8Path) -> anyhow::Result<Self> {
        if !path.is_absolute() {
            anyhow::bail!("can only edit absolute paths, got {path}");
        }
        let data = fs_err::read_to_string(path).context("Failed to read manifest contents")?;
        let manifest = data.parse().context("Unable to parse Cargo.toml")?;
        Ok(Self {
            manifest,
            path: path.to_owned(),
        })
    }

    /// Write changes back to the file
    pub fn write(&self) -> anyhow::Result<()> {
        let s = self.manifest.data.to_string();
        let new_contents_bytes = s.as_bytes();

        fs_err::write(&self.path, new_contents_bytes).context("Failed to write updated Cargo.toml")
    }

    /// Get the dependency tables of the package itself, including target-specific ones,
    /// with the kind of dependencies each table declares (e.g. [`DepKind::Development`]
    /// for `[dev-dependencies]` and `[target.'cfg(..)'.dev-dependencies]`).
    /// `[workspace.dependencies]` entries are templates, not dependencies of this package,
    /// so they are left out.
    pub fn get_package_dependency_tables(
        &self,
    ) -> impl Iterator<Item = (DepKind, &dyn toml_edit::TableLike)> + '_ {
        let root = self.data.as_table();
        root.iter().flat_map(|(key, v)| {
            let package_table = DepKind::from_table_name(key)
                .and_then(|kind| v.as_table_like().map(|table| (kind, table)));
            let target_tables = (key == "target")
                .then(|| v.as_table_like().unwrap())
                .into_iter()
                .flat_map(toml_edit::TableLike::iter)
                .flat_map(|(_, v)| {
                    v.as_table_like().into_iter().flat_map(|v| {
                        v.iter().filter_map(|(k, v)| {
                            let kind = DepKind::from_table_name(k)?;
                            v.as_table_like().map(|table| (kind, table))
                        })
                    })
                });
            package_table.into_iter().chain(target_tables)
        })
    }

    /// Allow mutating depedencies, wherever they live
    pub fn get_dependency_tables_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn toml_edit::TableLike> + '_ {
        let root = self.data.as_table_mut();
        let mut tables = Vec::new();
        for (k, v) in root.iter_mut() {
            if DepKind::from_table_name(k.get()).is_some() {
                tables.extend(v.as_table_like_mut());
            } else if k == "workspace" {
                tables.extend(
                    v.as_table_like_mut()
                        .unwrap()
                        .iter_mut()
                        .filter_map(|(k, v)| {
                            if k.get() == "dependencies" {
                                v.as_table_like_mut()
                            } else {
                                None
                            }
                        }),
                );
            } else if k == "target" {
                tables.extend(
                    v.as_table_like_mut()
                        .unwrap()
                        .iter_mut()
                        .flat_map(|(_, v)| {
                            v.as_table_like_mut().into_iter().flat_map(|v| {
                                v.iter_mut().filter_map(|(k, v)| {
                                    if DepKind::from_table_name(k.get()).is_some() {
                                        v.as_table_like_mut()
                                    } else {
                                        None
                                    }
                                })
                            })
                        }),
                );
            }
        }
        tables.into_iter()
    }

    /// Iterates over the `[workspace.dependencies]`.
    pub fn get_workspace_dependency_table(&self) -> Option<&dyn toml_edit::TableLike> {
        self.data
            .get("workspace")?
            .get("dependencies")?
            .as_table_like()
    }

    /// Override the manifest's version
    pub fn set_package_version(&mut self, version: &Version) {
        self.data["package"]["version"] = toml_edit::value(version.to_string());
    }

    /// `true` if the package inherits the workspace version
    pub fn version_is_inherited(&self) -> bool {
        fn inherits_workspace_version_impl(this: &Manifest) -> Option<bool> {
            this.data
                .get("package")?
                .get("version")?
                .get("workspace")?
                .as_bool()
        }

        inherits_workspace_version_impl(self).unwrap_or(false)
    }

    /// Get the current workspace version, if any.
    pub fn get_workspace_version(&self) -> Option<Version> {
        let version = self
            .data
            .get("workspace")?
            .get("package")?
            .get("version")?
            .as_str()?;
        Version::parse(version).ok()
    }

    /// Override the workspace's version.
    pub fn set_workspace_version(&mut self, version: &Version) {
        self.data["workspace"]["package"]["version"] = toml_edit::value(version.to_string());
    }
}

pub fn workspace_manifest(metadata: &Metadata) -> Utf8PathBuf {
    metadata.workspace_root.join(CARGO_TOML)
}

pub fn canonical_local_manifest(local_manifest: &Path) -> anyhow::Result<Utf8PathBuf> {
    let mut local_manifest = dunce::canonicalize(local_manifest)?;
    if !local_manifest.ends_with(CARGO_TOML) {
        local_manifest.push(CARGO_TOML);
    }
    let local_manifest = to_utf8_pathbuf(local_manifest)?;
    Ok(local_manifest)
}
