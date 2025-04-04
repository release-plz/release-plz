use std::{
    env,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use anyhow::Context;
use cargo_metadata::{
    Metadata,
    camino::{Utf8Path, Utf8PathBuf},
};
use semver::Version;

use crate::{CARGO_TOML, DepTable, Manifest, to_utf8_pathbuf};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FeatureStatus {
    None,
    DepFeature,
    Feature,
}

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
    /// Construct a `LocalManifest`. If no path is provided, make an educated guess as to which one
    /// the user means.
    pub fn find(path: Option<&Path>) -> anyhow::Result<Self> {
        let canonicalized_path = dunce::canonicalize(find(path)?)?;
        let path = to_utf8_pathbuf(canonicalized_path)?;
        Self::try_new(&path)
    }

    /// Construct the `LocalManifest` corresponding to the `Path` provided.
    pub fn try_new(path: &Utf8Path) -> anyhow::Result<Self> {
        if !path.is_absolute() {
            anyhow::bail!("can only edit absolute paths, got {}", path);
        }
        let data = fs_err::read_to_string(path).context("Failed to read manifest contents")?;
        let manifest = data.parse().context("Unable to parse Cargo.toml")?;
        Ok(LocalManifest {
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

    pub fn get_dependency_tables(&self) -> impl Iterator<Item = &dyn toml_edit::TableLike> + '_ {
        let root = self.data.as_table();
        root.iter().flat_map(|(key, v)| {
            if DepTable::KINDS.iter().any(|kind| kind.kind_table() == key) {
                v.as_table_like().into_iter().collect::<Vec<_>>()
            } else if key == "workspace" {
                v.as_table_like()
                    .unwrap()
                    .iter()
                    .filter_map(|(k, v)| {
                        if k == "dependencies" {
                            v.as_table_like()
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else if key == "target" {
                v.as_table_like()
                    .unwrap()
                    .iter()
                    .flat_map(|(_, v)| {
                        v.as_table_like().into_iter().flat_map(|v| {
                            v.iter().filter_map(|(k, v)| {
                                if DepTable::KINDS.iter().any(|kind| kind.kind_table() == k) {
                                    v.as_table_like()
                                } else {
                                    None
                                }
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
    }

    /// Allow mutating depedencies, wherever they live
    pub fn get_dependency_tables_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn toml_edit::TableLike> + '_ {
        let root = self.data.as_table_mut();
        root.iter_mut().flat_map(|(k, v)| {
            if DepTable::KINDS
                .iter()
                .any(|kind| kind.kind_table() == k.get())
            {
                v.as_table_like_mut().into_iter().collect::<Vec<_>>()
            } else if k == "workspace" {
                v.as_table_like_mut()
                    .unwrap()
                    .iter_mut()
                    .filter_map(|(k, v)| {
                        if k.get() == "dependencies" {
                            v.as_table_like_mut()
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else if k == "target" {
                v.as_table_like_mut()
                    .unwrap()
                    .iter_mut()
                    .flat_map(|(_, v)| {
                        v.as_table_like_mut().into_iter().flat_map(|v| {
                            v.iter_mut().filter_map(|(k, v)| {
                                if DepTable::KINDS
                                    .iter()
                                    .any(|kind| kind.kind_table() == k.get())
                                {
                                    v.as_table_like_mut()
                                } else {
                                    None
                                }
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
    }

    /// Iterates mutably over the `[workspace.dependencies]`.
    pub fn get_workspace_dependency_table_mut(&mut self) -> Option<&mut dyn toml_edit::TableLike> {
        self.data
            .get_mut("workspace")?
            .get_mut("dependencies")?
            .as_table_like_mut()
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

    /// Remove references to `dep_key` if its no longer present
    pub fn gc_dep(&mut self, dep_key: &str) {
        let status = self.dep_feature(dep_key);
        if matches!(status, FeatureStatus::None | FeatureStatus::DepFeature) {
            if let toml_edit::Item::Table(feature_table) = &mut self.data.as_table_mut()["features"]
            {
                for (_feature, mut activated_crates) in feature_table.iter_mut() {
                    if let toml_edit::Item::Value(toml_edit::Value::Array(feature_activations)) =
                        &mut activated_crates
                    {
                        remove_feature_activation(feature_activations, dep_key, status);
                    }
                }
            }
        }
    }

    fn dep_feature(&self, dep_key: &str) -> FeatureStatus {
        let mut status = FeatureStatus::None;
        for (_, tbl) in self.get_sections() {
            if let toml_edit::Item::Table(tbl) = tbl {
                if let Some(dep_item) = tbl.get(dep_key) {
                    let optional = dep_item.get("optional");
                    let optional = optional.and_then(|i| i.as_value());
                    let optional = optional.and_then(|i| i.as_bool());
                    let optional = optional.unwrap_or(false);
                    if optional {
                        return FeatureStatus::Feature;
                    } else {
                        status = FeatureStatus::DepFeature;
                    }
                }
            }
        }
        status
    }
}

/// If a manifest is specified, return that one, otherise perform a manifest search starting from
/// the current directory.
/// If a manifest is specified, return that one. If a path is specified, perform a manifest search
/// starting from there. If nothing is specified, start searching from the current directory
/// (`cwd`).
pub fn find(specified: Option<&Path>) -> anyhow::Result<PathBuf> {
    match specified {
        Some(path)
            if fs_err::metadata(path)
                .with_context(|| "Failed to get cargo file metadata")?
                .is_file() =>
        {
            Ok(path.to_owned())
        }
        Some(path) => find_manifest_path(path),
        None => find_manifest_path(&env::current_dir().context("Failed to get current directory")?),
    }
}

/// Search for Cargo.toml in this directory and recursively up the tree until one is found.
pub(crate) fn find_manifest_path(dir: &Path) -> anyhow::Result<PathBuf> {
    const MANIFEST_FILENAME: &str = "Cargo.toml";
    for path in dir.ancestors() {
        let manifest = path.join(MANIFEST_FILENAME);
        if fs_err::metadata(&manifest).is_ok() {
            return Ok(manifest);
        }
    }
    anyhow::bail!("Unable to find Cargo.toml for {}", dir.display());
}

fn remove_feature_activation(
    feature_activations: &mut toml_edit::Array,
    dep: &str,
    status: FeatureStatus,
) {
    let dep_feature: &str = &format!("{dep}/");

    let remove_list: Vec<usize> = feature_activations
        .iter()
        .enumerate()
        .filter_map(|(idx, feature_activation)| {
            if let toml_edit::Value::String(feature_activation) = feature_activation {
                let activation = feature_activation.value();
                match status {
                    FeatureStatus::None => activation == dep || activation.starts_with(dep_feature),
                    FeatureStatus::DepFeature => activation == dep,
                    FeatureStatus::Feature => false,
                }
                .then_some(idx)
            } else {
                None
            }
        })
        .collect();

    // Remove found idx in revers order so we don't invalidate the idx.
    for idx in remove_list.iter().rev() {
        feature_activations.remove(*idx);
    }
}

pub fn workspace_manifest(metadata: &Metadata) -> Utf8PathBuf {
    metadata.workspace_root.join("Cargo.toml")
}

pub fn canonical_local_manifest(local_manifest: &Path) -> anyhow::Result<Utf8PathBuf> {
    let mut local_manifest = dunce::canonicalize(local_manifest)?;
    if !local_manifest.ends_with(CARGO_TOML) {
        local_manifest.push(CARGO_TOML);
    }
    let local_manifest = to_utf8_pathbuf(local_manifest)?;
    Ok(local_manifest)
}
