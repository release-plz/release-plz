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

impl Drop for Utf8TempDir {
    /// Deleting a copy of a big repository one file at a time takes seconds,
    /// so delete the files from multiple threads first.
    /// [`tempfile::TempDir`] then removes the remaining directories.
    fn drop(&mut self) {
        // Symlinks aren't followed, so only the links themselves are deleted.
        ignore::WalkBuilder::new(&self.path)
            .standard_filters(false)
            .build_parallel()
            .run(|| {
                Box::new(|entry| {
                    if let Ok(entry) = entry
                        && entry
                            .file_type()
                            .is_some_and(|file_type| !file_type.is_dir())
                    {
                        // Best effort: `tempfile::TempDir` retries what's left.
                        drop(fs_err::remove_file(entry.path()));
                    }
                    ignore::WalkState::Continue
                })
            });
    }
}

// The test creates symlinks, which need privileges on Windows.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn dropping_temp_dir_keeps_symlink_targets() {
        let outside = Utf8TempDir::new().unwrap();
        let outside_file = outside.path().join("file");
        fs_err::write(&outside_file, "contents").unwrap();
        let temp_dir = Utf8TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();
        fs_err::create_dir(path.join("nested")).unwrap();
        fs_err::write(path.join("nested/file"), "contents").unwrap();
        std::os::unix::fs::symlink(outside.path(), path.join("dir_link")).unwrap();
        std::os::unix::fs::symlink(&outside_file, path.join("nested/file_link")).unwrap();

        drop(temp_dir);
        assert!(!path.exists());
        // Only the links are deleted, not what they point to.
        assert_eq!(fs_err::read_to_string(outside_file).unwrap(), "contents");
    }
}
