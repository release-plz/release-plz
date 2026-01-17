use std::path::Path;

use anyhow::Context;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_utils::to_utf8_pathbuf;

pub fn strip_prefix(path: &Utf8Path, prefix: impl AsRef<Path>) -> anyhow::Result<&Utf8Path> {
    path.strip_prefix(prefix.as_ref())
        .with_context(|| format!("cannot strip prefix {:?} from {:?}", prefix.as_ref(), path))
}

pub fn to_utf8_path(path: &Path) -> anyhow::Result<&Utf8Path> {
    Utf8Path::from_path(path).with_context(|| format!("cannot convert {path:?} to Utf8Path"))
}

pub fn current_directory() -> anyhow::Result<Utf8PathBuf> {
    to_utf8_pathbuf(std::env::current_dir().context("Unable to get current directory.")?)
}

pub fn canonicalize_utf8(path: &Utf8Path) -> anyhow::Result<Utf8PathBuf> {
    let canonicalized =
        dunce::canonicalize(path).with_context(|| format!("cannot canonicalize path {path:?}"))?;
    to_utf8_pathbuf(canonicalized)
}

#[derive(Debug)]
pub struct Utf8TempDir {
    // temporary directory that will be deleted in the `Drop` method
    _temp_dir: tempfile::TempDir,
    path: Utf8PathBuf,
}

impl Utf8TempDir {
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir().with_context(|| "cannot create temporary directory")?;
        let path = to_utf8_path(temp_dir.as_ref())?.to_path_buf();
        Ok(Self {
            _temp_dir: temp_dir,
            path,
        })
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }
}
