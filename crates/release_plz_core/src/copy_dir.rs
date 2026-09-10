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
    anyhow::ensure!(from.is_dir(), "not a directory: {from:?}");
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

    copy_directory(from, &to)?;

    Ok(())
}

/// `to` must exist.
#[tracing::instrument]
fn copy_directory(from: &Utf8Path, to: &Utf8Path) -> Result<(), anyhow::Error> {
    let walker = ignore::WalkBuilder::new(from)
        // Read hidden files
        .hidden(false)
        // Don't consider `.ignore` files.
        .ignore(false)
        // Ignore the global `.gitignore` as it might cause issues.
        // For example, if it contains `.git/`, we will fail in recognizing the git directory later.
        .git_global(false)
        // Skip the root `.git` entry (depth 1) and its descendants;
        // the separate walker below copies them without ignore filtering.
        .filter_entry(|entry| entry.depth() != 1 || entry.file_name() != ".git")
        .build();
    // Copy Git metadata without applying ignore rules: patterns such as `tags`
    // must not exclude `.git/refs/tags`. `.git` can also be a worktree's gitdir file.
    let git_path = from.join(".git");
    let git_walker = (git_path.try_exists()? || git_path.is_symlink()).then(|| {
        ignore::WalkBuilder::new(&git_path)
            .standard_filters(false)
            .build()
    });
    for entry in walker.chain(git_walker.into_iter().flatten()) {
        let entry = entry.context("invalid entry")?;
        let destination =
            destination_path(to, &entry, from).context("failed to determine destination path")?;
        let file_type = entry.file_type().context("unknown file type")?;
        copy_entry(from, to, entry.path().try_into()?, &destination, file_type)?;
    }
    copy_tracked_files(from, to)?;
    Ok(())
}

/// Ignore rules only apply to untracked files. The walker can skip whole ignored
/// directories, so copy any missing tracked paths directly from the index.
fn copy_tracked_files(from: &Utf8Path, to: &Utf8Path) -> anyhow::Result<()> {
    let repo = match git2::Repository::open(from) {
        Ok(repo) => repo,
        // This helper also copies directories that aren't Git repositories.
        Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).context("cannot open repository while copying tracked files");
        }
    };
    let index = repo
        .index()
        .context("cannot read index while copying tracked files")?;
    for entry in index.iter() {
        let relative = std::str::from_utf8(&entry.path).context("non-UTF-8 tracked path")?;
        let relative = Utf8Path::new(relative);
        // An ancestor replaced by a symlink makes this indexed path deleted.
        // symlink_metadata only avoids following symlinks at the final component.
        if relative
            .ancestors()
            .skip(1)
            .any(|ancestor| !ancestor.as_str().is_empty() && from.join(ancestor).is_symlink())
        {
            continue;
        }
        let source = from.join(relative);
        let metadata = match fs_err::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            // Preserve working-tree deletions rather than restoring index contents.
            // An ancestor replaced by a file also makes the tracked path absent.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let destination = to.join(relative);
        if metadata.is_dir() {
            // Gitlinks represent submodules, whose tracked files have their own index.
            if destination.try_exists()? {
                copy_tracked_files(&source, &destination)?;
            } else {
                fs_err::create_dir_all(&destination)?;
                copy_directory(&source, &destination)?;
            }
        } else {
            match fs_err::symlink_metadata(&destination) {
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            fs_err::create_dir_all(destination.parent().context("tracked path has no parent")?)?;
            copy_entry(from, to, &source, &destination, metadata.file_type())?;
        }
    }
    Ok(())
}

#[expect(clippy::filetype_is_file)] // we want to distinguish between files and symlinks
fn copy_entry(
    from: &Utf8Path,
    to: &Utf8Path,
    source: &Utf8Path,
    destination: &Utf8Path,
    file_type: std::fs::FileType,
) -> anyhow::Result<()> {
    if file_type.is_dir() {
        if destination != to {
            trace!("creating directory {:?}", destination);
            fs_err::create_dir_all(destination)?;
        }
    } else if file_type.is_symlink() {
        let original_link = Utf8Path::read_link_utf8(source)
            .with_context(|| format!("cannot read link {source:?}"))?;
        debug!("found symlink {:?} -> {:?}", source, original_link);
        let original_link = if original_link.is_relative() {
            original_link
        } else {
            let new_relative = strip_prefix(&original_link, from)?;
            to.join(new_relative)
        };
        create_symlink(&original_link, destination).with_context(|| {
            format!("cannot create symlink {original_link:?} -> {destination:?}")
        })?;
    } else if file_type.is_file() {
        trace!("copying file {:?} to {:?}", source, destination);
        fs_err::copy(source, destination)
            .with_context(|| format!("cannot copy file {source:?} to {destination:?}"))?;
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

    #[cfg(unix)]
    #[test]
    fn ignored_symlink_ancestors_preserve_tracked_deletions() {
        let source = Utf8TempDir::new().unwrap();
        let repo_dir = source.path().join("repo");
        fs_err::create_dir(&repo_dir).unwrap();
        let repo = git_cmd::Repo::init(&repo_dir);
        fs_err::create_dir_all(repo_dir.join("examples/nested")).unwrap();
        fs_err::write(repo_dir.join("examples/nested/tracked.txt"), "committed").unwrap();
        repo.add_all_and_commit("add tracked file").unwrap();
        fs_err::write(repo_dir.join(".gitignore"), "examples\n").unwrap();
        repo.add_all_and_commit("ignore examples").unwrap();

        let external_dir = source.path().join("external");
        fs_err::create_dir_all(external_dir.join("nested")).unwrap();
        fs_err::write(external_dir.join("nested/tracked.txt"), "external contents").unwrap();
        fs_err::remove_dir_all(repo_dir.join("examples")).unwrap();
        create_symlink("../external", repo_dir.join("examples")).unwrap();
        let source_status = repo.git(&["status", "--porcelain"]).unwrap();
        assert_eq!(source_status, "D examples/nested/tracked.txt");

        let destination = Utf8TempDir::new().unwrap();
        copy_dir(&repo_dir, destination.path()).unwrap();
        let copied_dir = destination.path().join("repo");
        let copied_repo = git_cmd::Repo::new(&copied_dir).unwrap();
        assert_eq!(
            copied_repo.git(&["status", "--porcelain"]).unwrap(),
            source_status
        );
        assert!(!copied_dir.join("examples").exists());
    }

    #[test]
    fn tracked_files_are_copied_despite_ignore_rules() {
        for (ignore_file, pattern) in [
            (".gitignore", "Cargo.lock"),
            (".gitignore", "examples/"),
            (".git/info/exclude", "examples/"),
        ] {
            let source = Utf8TempDir::new().unwrap();
            let repo_dir = source.path().join("repo");
            fs_err::create_dir(&repo_dir).unwrap();
            let repo = git_cmd::Repo::init(&repo_dir);
            fs_err::create_dir(repo_dir.join("examples")).unwrap();
            for path in ["examples/Cargo.lock", "examples/space and [brackets].txt"] {
                fs_err::write(repo_dir.join(path), "tracked contents").unwrap();
            }
            repo.add_all_and_commit("add tracked files").unwrap();
            fs_err::write(repo_dir.join(ignore_file), format!("{pattern}\ntarget/\n")).unwrap();
            if ignore_file == ".gitignore" {
                repo.add_all_and_commit("ignore generated files").unwrap();
            }
            fs_err::create_dir(repo_dir.join("target")).unwrap();
            fs_err::write(repo_dir.join("target/output"), "ignored").unwrap();
            fs_err::create_dir(repo_dir.join("examples/untracked")).unwrap();
            fs_err::write(repo_dir.join("examples/untracked/Cargo.lock"), "ignored").unwrap();
            assert_eq!(repo.git(&["status", "--porcelain"]).unwrap(), "");

            let destination = Utf8TempDir::new().unwrap();
            copy_dir(&repo_dir, destination.path()).unwrap();
            let copied_dir = destination.path().join("repo");
            let copied_repo = git_cmd::Repo::new(&copied_dir).unwrap();
            assert_eq!(copied_repo.git(&["status", "--porcelain"]).unwrap(), "");
            for path in ["examples/Cargo.lock", "examples/space and [brackets].txt"] {
                assert_eq!(
                    fs_err::read_to_string(copied_dir.join(path)).unwrap(),
                    "tracked contents"
                );
            }
            assert!(!copied_dir.join("target").exists());
            assert!(!copied_dir.join("examples/untracked/Cargo.lock").exists());
        }
    }

    #[test]
    fn ignored_tracked_files_preserve_working_tree_changes() {
        let source = Utf8TempDir::new().unwrap();
        let repo_dir = source.path().join("repo");
        fs_err::create_dir(&repo_dir).unwrap();
        let repo = git_cmd::Repo::init(&repo_dir);
        fs_err::create_dir(repo_dir.join("examples")).unwrap();
        fs_err::write(repo_dir.join("examples/Cargo.lock"), "committed").unwrap();
        fs_err::write(repo_dir.join("examples/deleted"), "committed").unwrap();
        create_symlink("missing", repo_dir.join("examples/link")).unwrap();
        repo.add_all_and_commit("add tracked files").unwrap();
        fs_err::write(repo_dir.join(".gitignore"), "examples/\n").unwrap();
        repo.add_all_and_commit("ignore examples").unwrap();
        fs_err::write(repo_dir.join("examples/Cargo.lock"), "modified").unwrap();
        fs_err::remove_file(repo_dir.join("examples/deleted")).unwrap();
        fs_err::write(repo_dir.join("examples/staged"), "staged").unwrap();
        repo.git(&["add", "-f", "examples/staged"]).unwrap();
        fs_err::write(repo_dir.join("examples/staged"), "modified after staging").unwrap();

        let destination = Utf8TempDir::new().unwrap();
        copy_dir(&repo_dir, destination.path()).unwrap();
        let copied_dir = destination.path().join("repo");
        let copied_repo = git_cmd::Repo::new(&copied_dir).unwrap();
        assert_eq!(
            copied_repo.git(&["status", "--porcelain"]).unwrap(),
            repo.git(&["status", "--porcelain"]).unwrap()
        );
        assert_eq!(
            fs_err::read_to_string(copied_dir.join("examples/Cargo.lock")).unwrap(),
            "modified"
        );
        assert_eq!(
            fs_err::read_to_string(copied_dir.join("examples/staged")).unwrap(),
            "modified after staging"
        );
        assert!(!copied_dir.join("examples/deleted").exists());
        assert_eq!(
            fs_err::read_link(copied_dir.join("examples/link")).unwrap(),
            Path::new("missing")
        );
    }

    #[test]
    fn tracked_directory_replaced_by_file_is_copied() {
        let source = Utf8TempDir::new().unwrap();
        let repo_dir = source.path().join("repo");
        fs_err::create_dir(&repo_dir).unwrap();
        let repo = git_cmd::Repo::init(&repo_dir);
        fs_err::create_dir(repo_dir.join("examples")).unwrap();
        fs_err::write(repo_dir.join("examples/tracked.txt"), "committed").unwrap();
        repo.add_all_and_commit("add tracked file").unwrap();
        fs_err::remove_dir_all(repo_dir.join("examples")).unwrap();
        fs_err::write(repo_dir.join("examples"), "replacement").unwrap();

        let destination = Utf8TempDir::new().unwrap();
        copy_dir(&repo_dir, destination.path()).unwrap();
        let copied_dir = destination.path().join("repo");
        let copied_repo = git_cmd::Repo::new(&copied_dir).unwrap();
        assert_eq!(
            fs_err::read_to_string(copied_dir.join("examples")).unwrap(),
            "replacement"
        );
        assert_eq!(
            copied_repo.git(&["status", "--porcelain"]).unwrap(),
            repo.git(&["status", "--porcelain"]).unwrap()
        );
    }

    #[test]
    fn ignored_tracked_submodule_files_are_copied() {
        for pattern in ["", "examples/\n"] {
            let source = Utf8TempDir::new().unwrap();
            let submodule_dir = source.path().join("submodule");
            fs_err::create_dir(&submodule_dir).unwrap();
            let submodule = git_cmd::Repo::init(&submodule_dir);
            fs_err::write(submodule_dir.join("Cargo.lock"), "tracked lockfile").unwrap();
            create_symlink("missing", submodule_dir.join("link")).unwrap();
            submodule.add_all_and_commit("add tracked files").unwrap();
            fs_err::write(submodule_dir.join(".gitignore"), "Cargo.lock\ntarget/\n").unwrap();
            submodule.add_all_and_commit("ignore build files").unwrap();

            let repo_dir = source.path().join("repo");
            fs_err::create_dir(&repo_dir).unwrap();
            let repo = git_cmd::Repo::init(&repo_dir);
            repo.git(&[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                submodule_dir.as_str(),
                "examples",
            ])
            .unwrap();
            fs_err::write(repo_dir.join(".gitignore"), pattern).unwrap();
            repo.add_all_and_commit("add submodule").unwrap();
            fs_err::create_dir(repo_dir.join("examples/target")).unwrap();
            fs_err::write(repo_dir.join("examples/target/output"), "ignored").unwrap();

            let destination = Utf8TempDir::new().unwrap();
            copy_dir(&repo_dir, destination.path()).unwrap();
            let copied_dir = destination.path().join("repo");
            let copied_repo = git_cmd::Repo::new(&copied_dir).unwrap();
            assert_eq!(copied_repo.git(&["status", "--porcelain"]).unwrap(), "");
            assert_eq!(
                fs_err::read_to_string(copied_dir.join("examples/Cargo.lock")).unwrap(),
                "tracked lockfile"
            );
            assert_eq!(
                fs_err::read_link(copied_dir.join("examples/link")).unwrap(),
                Path::new("missing")
            );
            assert!(!copied_dir.join("examples/target").exists());
        }
    }

    #[test]
    fn git_metadata_is_copied_despite_ignore_rules() {
        for patterns in [
            "tags",
            "tags\nlogs\nconfig\nindex\nobjects\ninfo\nhooks\ndescription\nrefs\nHEAD\npacked-refs",
            ".git\ntags",
        ] {
            let source = Utf8TempDir::new().unwrap();
            let repo_dir = source.path().join("repo");
            fs_err::create_dir(&repo_dir).unwrap();
            let repo = git_cmd::Repo::init(&repo_dir);
            repo.tag("v0.1.0", "Release v0.1.0").unwrap();
            repo.git(&["pack-refs", "--all"]).unwrap();
            repo.tag_lightweight("v0.2.0").unwrap();
            fs_err::write(repo_dir.join(".gitignore"), format!("{patterns}\ntarget\n")).unwrap();
            fs_err::write(repo_dir.join("tags"), "ctags index").unwrap();
            fs_err::create_dir(repo_dir.join("target")).unwrap();
            fs_err::write(repo_dir.join("target/build-output"), "ignored").unwrap();

            let destination = Utf8TempDir::new().unwrap();
            copy_dir(&repo_dir, destination.path()).unwrap();
            let copied_dir = destination.path().join("repo");
            let copied_repo = git_cmd::Repo::new(&copied_dir).unwrap();
            assert_eq!(
                copied_repo.get_all_tags(),
                repo.get_all_tags(),
                "{patterns}"
            );
            for tag in ["v0.1.0", "v0.2.0"] {
                assert_eq!(copied_repo.get_tag_commit(tag), repo.get_tag_commit(tag));
            }
            // Every metadata file must survive, including the index, config and objects.
            for entry in walkdir::WalkDir::new(repo_dir.join(".git")) {
                let entry = entry.unwrap();
                let relative = entry.path().strip_prefix(&repo_dir).unwrap();
                let copied = copied_dir.join(Utf8Path::from_path(relative).unwrap());
                if entry.file_type().is_dir() {
                    assert!(copied.is_dir(), "missing {copied} with {patterns}");
                } else {
                    assert_eq!(
                        fs_err::read(entry.path()).unwrap(),
                        fs_err::read(copied).unwrap()
                    );
                }
            }
            assert!(copied_dir.join("README.md").exists());
            assert!(copied_dir.join(".gitignore").exists());
            assert!(!copied_dir.join("tags").exists());
            assert!(!copied_dir.join("target").exists());
        }
    }

    #[test]
    fn git_file_is_copied_despite_ignore_rules() {
        let source = Utf8TempDir::new().unwrap();
        let repo_dir = source.path().join("repo");
        fs_err::create_dir(&repo_dir).unwrap();
        fs_err::write(
            repo_dir.join(".git"),
            "gitdir: ../main/.git/worktrees/repo\n",
        )
        .unwrap();
        fs_err::write(repo_dir.join(".gitignore"), ".git\n").unwrap();

        let destination = Utf8TempDir::new().unwrap();
        copy_dir(&repo_dir, destination.path()).unwrap();
        assert_eq!(
            fs_err::read(destination.path().join("repo/.git")).unwrap(),
            fs_err::read(repo_dir.join(".git")).unwrap()
        );
    }

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
