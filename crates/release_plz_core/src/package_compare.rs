use anyhow::Context;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
};
use cargo_utils::{CARGO_TOML, get_manifest_metadata};
use tracing::{debug, info};

use crate::{cargo::run_cargo, fs_utils};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{self, Read},
    path::Path,
};

/// Check if two packages are equal.
///
/// ## Args
/// - `ignored_dirs`: Directories of the `local_package` to ignore when comparing packages.
pub fn are_packages_equal(
    local_package: &Utf8Path,
    registry_package: &Utf8Path,
) -> anyhow::Result<bool> {
    debug!(
        "compare local package {:?} with registry package {:?}",
        local_package, registry_package
    );
    if !are_cargo_toml_equal(local_package, registry_package) {
        return Ok(false);
    }

    // When a package is published to a cargo registry, the original `Cargo.toml` file is stored as `Cargo.toml.orig`.
    // We need to rename it to `Cargo.toml.orig.orig`, because this name is reserved, and `cargo package` will fail if it exists.
    rename(
        registry_package.join("Cargo.toml.orig"),
        registry_package.join("Cargo.toml.orig.orig"),
    )?;

    let local_package_files = get_cargo_package_files(local_package).with_context(|| {
        format!("cannot determine packaged files of local package {local_package:?}")
    })?;
    let registry_package_files = get_cargo_package_files(registry_package).with_context(|| {
        format!("cannot determine packaged files of registry package {registry_package:?}")
    })?;

    // Rename the file to the original name.
    rename(
        registry_package.join("Cargo.toml.orig.orig"),
        registry_package.join("Cargo.toml.orig"),
    )?;

    let local_files = local_package_files.iter().filter(|file| {
        *file != "Cargo.toml.orig" && *file != ".cargo_vcs_info.json" && *file != "Cargo.lock"
    });

    let registry_files = registry_package_files.iter().filter(|file| {
        *file != "Cargo.toml.orig"
            && *file != "Cargo.toml.orig.orig"
            && *file != ".cargo_vcs_info.json"
            && *file != "Cargo.lock"
            && *file != ".cargo-ok"
    });

    if !local_files.clone().eq(registry_files) {
        // New files were added or removed.
        return Ok(false);
    }

    let local_files = local_files
        .map(|file| local_package.join(file))
        .filter(|file| {
            !(file.is_symlink()
            // `cargo package --list` can return files that don't exist locally,
            // such as the `README.md` file if the `Cargo.toml` specified a different path.
            || !file.exists()
            // Ignore `Cargo.lock` because the local one is different from the published one in workspaces.
            || file.file_name() == Some("Cargo.lock")
            // Ignore `Cargo.toml` because we already checked it before.
            || file.file_name() == Some(CARGO_TOML)
            // Ignore `Cargo.toml.orig` because it's auto generated.
            || file.file_name() == Some("Cargo.toml.orig"))
        });

    for local_path in local_files {
        let relative_path = local_path
            .strip_prefix(local_package)
            .with_context(|| format!("can't find {local_package:?} prefix in {local_path:?}"))?;

        let registry_path = registry_package.join(relative_path);
        if !are_files_equal(&local_path, &registry_path).context("files are not equal")? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> anyhow::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs_err::rename(from, to).with_context(|| format!("cannot rename {from:?} to {to:?}"))
}

pub fn get_cargo_package_files(package: &Utf8Path) -> anyhow::Result<Vec<Utf8PathBuf>> {
    // If Cargo.toml.orig (or its renamed variant Cargo.toml.orig.orig -- see
    // `are_packages_equal`) is present, this package's on-disk contents already
    // reflect exactly what `cargo package` would produce. This is true both for
    // packages extracted from a registry, and for ones built locally via
    // `cargo package` (e.g. in the git_only flow). In that case we can list files
    // directly from disk instead of invoking `cargo package`, which is slower and
    // can fail if the directory doesn't have full cargo metadata set up.
    // See https://github.com/release-plz/release-plz/issues/2130
    info!("Getting packaged files for crate at {}", package);
    if package.join("Cargo.toml.orig").exists() || package.join("Cargo.toml.orig.orig").exists() {
        let list =
            list_packaged_files(package).context("cannot list packaged files from directory")?;
        debug!("Packaged files: {:?}", list);
        Ok(list)
    } else {
        let list = get_cargo_package_list(package)
            .context("cannot get packaged files from cargo package list")?;
        debug!("Cargo Packaged files: {:?}", list);
        Ok(list)
    }
}

fn get_cargo_package_list(package: &Utf8Path) -> Result<Vec<Utf8PathBuf>, anyhow::Error> {
    // We use `--allow-dirty` because we have `Cargo.toml.orig.orig`, which is an uncommitted change.
    let args = ["package", "--list", "--quiet", "--allow-dirty"];
    let output = run_cargo(package, &args).context("cannot run `cargo package`")?;

    anyhow::ensure!(
        output.status.success(),
        "error while running `cargo package`: {}",
        output.stderr
    );

    let files = output.stdout.lines().map(Utf8PathBuf::from).collect();
    Ok(files)
}

fn list_packaged_files(package: &Utf8Path) -> anyhow::Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    let mut dirs = vec![package.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        for entry in fs_err::read_dir(&dir).with_context(|| format!("cannot read dir {dir:?}"))? {
            let entry = entry.with_context(|| format!("cannot read dir entry in {dir:?}"))?;
            let path = Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|path| anyhow::anyhow!("non-utf8 path in package: {path:?}"))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("cannot read file type for {path:?}"))?;

            if file_type.is_dir() {
                // Registry sources extracted via `git clone` (as some registries do) leave
                // a full `.git` directory behind. `cargo package --list` never includes
                // `.git`, so the disk-listing fast path must skip it too, or every file
                // inside it would make the registry package look different from the
                // local one for reasons unrelated to the actual packaged contents.
                if path.file_name() == Some(".git") {
                    continue;
                }
                dirs.push(path);
            } else {
                let rel_path = path
                    .strip_prefix(package)
                    .with_context(|| format!("can't find {package:?} prefix in {path:?}"))?;
                files.push(rel_path.to_path_buf());
            }
        }
    }

    files.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(files)
}

fn are_cargo_toml_equal(local_package: &Utf8Path, registry_package: &Utf8Path) -> bool {
    // When a package is published to a cargo registry, the original `Cargo.toml` file is stored as
    // `Cargo.toml.orig`
    let cargo_orig = format!("{CARGO_TOML}.orig");
    are_files_equal(
        &local_package.join(CARGO_TOML),
        &registry_package.join(cargo_orig),
    )
    .unwrap_or(false)
}

/// Returns true if the README file of the local package is the same as the one in the registry.
/// Returns false if:
/// - the README is the same
/// - the local package doesn't have a `readme` field in the `Cargo.toml`.
/// - the package doesn't have a README at all.
pub fn is_readme_updated(
    package_name: &str,
    local_package_path: &Utf8Path,
    registry_package_path: &Utf8Path,
) -> anyhow::Result<bool> {
    // Read again manifest metadata because the Cargo.toml might change on every commit.
    let package = match read_package_metadata(package_name, local_package_path) {
        Ok(package) => package,
        Err(e) => {
            tracing::warn!(
                "cannot read package metadata of {package_name} in {local_package_path}: {e:?}"
            );
            return Ok(false);
        }
    };

    let local_package_readme_path = local_readme_override(&package, local_package_path);
    let are_readmes_equal = match local_package_readme_path? {
        Some(local_package_readme_path) => {
            let registry_package_readme_path = registry_package_path.join("README.md");
            if !registry_package_readme_path.exists() {
                return Ok(true);
            }
            match are_files_equal(&local_package_readme_path, &registry_package_readme_path) {
                Ok(are_readmes_equal) => are_readmes_equal,
                Err(e) => {
                    tracing::warn!("cannot compare README files: {e}");
                    true
                }
            }
        }
        None => true,
    };
    Ok(!are_readmes_equal)
}

pub fn local_readme_override(
    package: &Package,
    local_package_path: &Utf8Path,
) -> anyhow::Result<Option<Utf8PathBuf>> {
    package
        .readme
        .as_ref()
        .and_then(|readme| {
            let readme_path = local_package_path.join(readme);
            if !readme_path.exists() {
                tracing::warn!(
                    "README path '{}' doesn't exist for package '{}'. Hint: ensure the path set in Cargo.toml points to a file that exists and is included in the crate.",
                    readme_path,
                    package.name
                );
                return None;
            }
            Some(fs_utils::canonicalize_utf8(&readme_path))
        })
        .transpose()
}

fn are_files_equal(first: &Utf8Path, second: &Utf8Path) -> anyhow::Result<bool> {
    let hash1 = file_hash(first).with_context(|| format!("cannot determine hash of {first:?}"))?;
    let hash2 =
        file_hash(second).with_context(|| format!("cannot determine hash of {second:?}"))?;
    Ok(hash1 == hash2)
}

fn file_hash(file: &Utf8Path) -> io::Result<u64> {
    let buffer = &mut vec![];
    fs_err::File::open(file)?.read_to_end(buffer)?;
    let mut hasher = DefaultHasher::new();
    buffer.hash(&mut hasher);
    let hash = hasher.finish();
    Ok(hash)
}

fn read_package_metadata(
    package_name: &str,
    local_package_path: &Utf8Path,
) -> anyhow::Result<Package> {
    let package = get_manifest_metadata(&local_package_path.join(CARGO_TOML))
        .context("cannot read Cargo.toml")?
        .workspace_packages()
        .into_iter()
        .find(|&p| *p.name == package_name)
        .cloned()
        .context("cannot find package in Cargo.toml")?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Registry-extracted packages (which don't live under `target/package`)
    /// should use the fast, disk-listing path when `Cargo.toml.orig` is
    /// present, instead of shelling out to `cargo package --list`.
    /// Regression test for
    /// <https://github.com/release-plz/release-plz/issues/2130>
    #[test]
    fn get_cargo_package_files_uses_disk_listing_when_cargo_toml_orig_present() {
        let dir = tempfile::tempdir().expect("cannot create tempdir");
        let package = Utf8Path::from_path(dir.path()).expect("non-utf8 tempdir path");

        // Deliberately do NOT write a valid Cargo.toml manifest. If
        // `get_cargo_package_files` fell through to invoking
        // `cargo package --list`, it would fail here since there's no valid
        // cargo manifest in this directory. The fact that it succeeds and
        // returns exactly the files we wrote proves the disk-listing fast
        // path was taken -- without this needing to be a real cargo project
        // or to live under target/package.
        fs::write(
            package.join("Cargo.toml.orig"),
            "this is not a real manifest",
        )
        .expect("cannot write Cargo.toml.orig");
        fs::create_dir(package.join("src")).expect("cannot create src dir");
        fs::write(package.join("src/lib.rs"), "").expect("cannot write lib.rs");

        let mut files = get_cargo_package_files(package).expect("should use disk listing");
        files.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        assert_eq!(
            files,
            vec![
                Utf8PathBuf::from("Cargo.toml.orig"),
                Utf8PathBuf::from("src/lib.rs")
            ]
        );
    }

    /// Same as above, but for the `Cargo.toml.orig.orig` marker used while
    /// `are_packages_equal` has temporarily renamed the registry package's
    /// `Cargo.toml.orig` out of the way.
    #[test]
    fn get_cargo_package_files_uses_disk_listing_when_cargo_toml_orig_orig_present() {
        let dir = tempfile::tempdir().expect("cannot create tempdir");
        let package = Utf8Path::from_path(dir.path()).expect("non-utf8 tempdir path");

        fs::write(
            package.join("Cargo.toml.orig.orig"),
            "this is not a real manifest",
        )
        .expect("cannot write Cargo.toml.orig.orig");

        let files = get_cargo_package_files(package).expect("should use disk listing");
        assert_eq!(files, vec![Utf8PathBuf::from("Cargo.toml.orig.orig")]);
    }

    /// Some registries (e.g. this environment's Gitea cargo registry) extract
    /// packages via `git clone`, leaving a full `.git` directory inside the
    /// extracted package. `cargo package --list` always ignores `.git`, so the
    /// disk-listing fast path must ignore it too, or every file inside it would
    /// make a registry-extracted package look different from the local one for
    /// reasons unrelated to the actual packaged contents.
    /// Regression test for <https://github.com/release-plz/release-plz/issues/2130>.
    #[test]
    fn get_cargo_package_files_ignores_nested_git_dir() {
        let dir = tempfile::tempdir().expect("cannot create tempdir");
        let package = Utf8Path::from_path(dir.path()).expect("non-utf8 tempdir path");

        fs::write(
            package.join("Cargo.toml.orig"),
            "this is not a real manifest",
        )
        .expect("cannot write Cargo.toml.orig");
        fs::create_dir(package.join("src")).expect("cannot create src dir");
        fs::write(package.join("src/lib.rs"), "").expect("cannot write lib.rs");

        // Simulate a nested `.git` directory left behind by a git-clone-based
        // registry extraction, with content that would trip up the comparison
        // if it weren't filtered out.
        fs::create_dir_all(package.join(".git/objects")).expect("cannot create .git dir");
        fs::write(package.join(".git/HEAD"), "ref: refs/heads/main").expect("cannot write .git/HEAD");
        fs::write(
            package.join(".git/objects/pack-dummy"),
            "binary-ish content",
        )
        .expect("cannot write .git/objects/pack-dummy");

        let mut files = get_cargo_package_files(package).expect("should use disk listing");
        files.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        assert_eq!(
            files,
            vec![
                Utf8PathBuf::from("Cargo.toml.orig"),
                Utf8PathBuf::from("src/lib.rs")
            ]
        );
    }
}
