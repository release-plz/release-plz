use std::{io, path::Path};

use anyhow::Context;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use tracing::{debug, trace};

use crate::fs_utils::strip_prefix;

pub(crate) fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(
    original: P,
    link: Q,
) -> io::Result<()> {
    debug!(
        "creating symlink {:?} -> {:?}",
        &original.as_ref(),
        &link.as_ref()
    );

    #[cfg(unix)]
    return std::os::unix::fs::symlink(original, link);

    #[cfg(windows)]
    return std::os::windows::fs::symlink_file(original, link);
}

/// Copy directory preserving symlinks.
/// `to` is created if it doesn't exist.
pub fn copy_dir(from: impl AsRef<Utf8Path>, to: impl AsRef<Utf8Path>) -> anyhow::Result<()> {
    let from = from.as_ref();
    anyhow::ensure!(from.is_dir(), "not a directory: {:?}", from);
    let dir_name = from
        .components()
        .next_back()
        .with_context(|| format!("invalid path {from:?}"))?;
    let to = to.as_ref().join(dir_name);
    debug!("copying directory from {:?} to {:?}", from, to);
    if !to.exists() {
        trace!("creating directory {:?}", to);
        fs_err::create_dir_all(&to)?;
    }

    copy_directory(from, to)?;

    Ok(())
}

/// `to` must exist.
#[tracing::instrument]
#[expect(clippy::filetype_is_file)] // we want to distinguish between files and symlinks
fn copy_directory(from: &Utf8Path, to: Utf8PathBuf) -> Result<(), anyhow::Error> {
    let walker = ignore::WalkBuilder::new(from)
        // Read hidden files
        .hidden(false)
        // Don't consider `.ignore` files.
        .ignore(false)
        // Ignore the global `.gitignore` as it might cause issues.
        // For example, if it contains `.git/`, we will fail in recognizing the git directory later.
        .git_global(false)
        .build();
    for entry in walker {
        let entry = entry.context("invalid entry")?;
        let destination =
            destination_path(&to, &entry, from).context("failed to determine destination path")?;
        let file_type = entry.file_type().context("unknown file type")?;
        if file_type.is_dir() {
            if destination == to {
                continue;
            }
            trace!("creating directory {:?}", destination);
            fs_err::create_dir(&destination)?;
        } else if file_type.is_symlink() {
            let entry_utf8: &Utf8Path = entry.path().try_into()?;
            let original_link = Utf8Path::read_link_utf8(entry_utf8)
                .with_context(|| format!("cannot read link {:?}", entry.path()))?;
            debug!("found symlink {:?} -> {:?}", entry.path(), original_link);
            let original_link = if original_link.is_relative() {
                original_link
            } else {
                let new_relative = strip_prefix(&original_link, from)?;
                to.join(new_relative)
            };
            create_symlink(&original_link, &destination).with_context(|| {
                format!(
                    "cannot create symlink {:?} -> {:?}",
                    &original_link, &destination
                )
            })?;
        } else if file_type.is_file() {
            trace!("copying file {:?} to {:?}", entry.path(), &destination);
            fs_err::copy(entry.path(), &destination).with_context(|| {
                format!("cannot copy file {:?} to {:?}", entry.path(), &destination)
            })?;
        }
    }
    Ok(())
}

fn destination_path(
    to: &Utf8Path,
    entry: &ignore::DirEntry,
    from: &Utf8Path,
) -> anyhow::Result<Utf8PathBuf> {
    let mut dest_path = to.to_path_buf();
    let relative = strip_prefix(entry.path().try_into()?, from)?;
    dest_path.push(relative);
    Ok(dest_path)
}

#[cfg(test)]
mod tests {
    use crate::fs_utils::Utf8TempDir;

    use super::*;

    #[test]
    fn is_dir_copied_correctly() {
        let temp = Utf8TempDir::new().unwrap();
        let subdir = "subdir";
        let subdir_path = temp.path().join(subdir);
        fs_err::create_dir(&subdir_path).unwrap();

        let file1 = subdir_path.join("file1");
        fs_err::write(&file1, "aaa").unwrap();
        let file2 = subdir_path.join("file2");
        create_symlink(&file1, file2).unwrap();

        let temp2 = Utf8TempDir::new().unwrap();
        copy_dir(subdir_path, temp2.path()).unwrap();
        let temp2_subdir = temp2.path().join(subdir);
        let new_file2 = temp2_subdir.join("file2");
        assert!(fs_err::symlink_metadata(&new_file2).unwrap().is_symlink());
        let link_target = fs_err::read_link(new_file2).unwrap();
        let file1_dest = temp2.path().join(subdir).join("file1");
        assert!(file1_dest.exists());
        assert_eq!(link_target, file1_dest);
    }

    #[test]
    fn is_symlink_created_if_file_exists() {
        let temp = tempfile::tempdir().unwrap();
        let file1 = temp.path().join("file1");
        let file2 = temp.path().join("file2");

        // file already exists
        fs_err::write(&file1, "aaa").unwrap();
        create_symlink(&file1, &file2).unwrap();
        let metadata = fs_err::symlink_metadata(&file2).unwrap();
        assert!(metadata.is_symlink());
        dbg!(metadata);
        let target = fs_err::read_link(file2).unwrap();
        assert_eq!(target, file1);
        assert_eq!(fs_err::read_to_string(target).unwrap(), "aaa");
        assert_eq!(fs_err::read_to_string(file1).unwrap(), "aaa");
    }

    #[test]
    fn is_symlink_created_before_file_exists() {
        let temp = tempfile::tempdir().unwrap();
        let file1 = temp.path().join("file1");
        let file2 = temp.path().join("file2");

        // file doesn't exist yet
        create_symlink(&file1, &file2).unwrap();
        fs_err::write(&file1, "aaa").unwrap();
        let metadata = fs_err::symlink_metadata(&file2).unwrap();
        assert!(metadata.is_symlink());
        dbg!(metadata);
        let target = fs_err::read_link(file2).unwrap();
        assert_eq!(target, file1);
        assert_eq!(fs_err::read_to_string(target).unwrap(), "aaa");
        assert_eq!(fs_err::read_to_string(file1).unwrap(), "aaa");
    }
}
