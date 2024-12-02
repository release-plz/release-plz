use cargo_metadata::camino::Utf8Path;
use git_cmd::Repo;
use std::ops::Deref;

use crate::fs_utils::Utf8TempDir;

pub struct TempRepo {
    // temporary directory that will be deleted in the `Drop` method
    _temp_dir: Utf8TempDir,
    pub repo: Repo,
}

impl TempRepo {
    pub fn new(temp_dir: Utf8TempDir, directory: impl AsRef<Utf8Path>) -> anyhow::Result<Self> {
        Ok(Self {
            _temp_dir: temp_dir,
            repo: Repo::new(directory)?,
        })
    }
}

impl Deref for TempRepo {
    type Target = Repo;

    fn deref(&self) -> &Self::Target {
        &self.repo
    }
}
