use anyhow::anyhow;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
};

use crate::fs_utils;

pub trait PackagePath {
    fn package_path(&self) -> anyhow::Result<&Utf8Path>;

    fn canonical_path(&self) -> anyhow::Result<Utf8PathBuf> {
        let p = fs_utils::canonicalize_utf8(self.package_path()?)?;
        Ok(p)
    }
}

impl PackagePath for Package {
    fn package_path(&self) -> anyhow::Result<&Utf8Path> {
        manifest_dir(&self.manifest_path)
    }
}

pub fn manifest_dir(manifest: &Utf8Path) -> anyhow::Result<&Utf8Path> {
    let manifest_dir = manifest.parent().ok_or_else(|| {
        anyhow!(
            "Cannot find directory where manifest {:?} is located",
            manifest
        )
    })?;
    Ok(manifest_dir)
}
