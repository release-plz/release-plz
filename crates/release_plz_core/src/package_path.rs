use anyhow::anyhow;
use cargo_metadata::{
    camino::{Utf8Path, Utf8PathBuf},
    Package,
};

pub trait PackagePath {
    fn package_path(&self) -> anyhow::Result<&Utf8Path>;

    fn canonical_path(&self) -> anyhow::Result<Utf8PathBuf> {
        let p = dunce::canonicalize(self.package_path()?)?.try_into()?;
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
