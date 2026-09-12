use anyhow::Context;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
};
use cargo_utils::CARGO_TOML;
use tracing::debug;

use crate::{
    cargo::{read_package_metadata, run_cargo},
    fs_utils,
};
use std::{
    cell::OnceCell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{self, Read},
};

/// The packaged files of the released package, computed at most once.
///
/// While walking the git history, the local package is checked out at a different
/// commit on every iteration, but the released package never changes. Listing its
/// files can mean running `cargo package --list`, which resolves dependencies and
/// can reach the registry index, so it must not run once per analyzed commit.
#[derive(Default)]
pub(crate) struct ReleasedPackageFiles(OnceCell<Vec<Utf8PathBuf>>);

impl ReleasedPackageFiles {
    fn get(&self, package: &Utf8Path) -> anyhow::Result<&[Utf8PathBuf]> {
        if let Some(files) = self.0.get() {
            return Ok(files);
        }
        let files = get_cargo_package_files(package).with_context(|| {
            format!("cannot determine packaged files of registry package {package:?}")
        })?;
        Ok(self.0.get_or_init(|| files))
    }
}

/// Check if two packages are equal.
pub fn are_packages_equal(
    local_package: &Utf8Path,
    registry_package: &Utf8Path,
) -> anyhow::Result<bool> {
    are_packages_equal_cached(local_package, registry_package, &ReleasedPackageFiles::default())
}

/// Same as [`are_packages_equal`], reusing the released package's file list
/// across the commits of a single history walk.
pub(crate) fn are_packages_equal_cached(
    local_package: &Utf8Path,
    registry_package: &Utf8Path,
    registry_package_files: &ReleasedPackageFiles,
) -> anyhow::Result<bool> {
    debug!(
        "compare local package {:?} with registry package {:?}",
        local_package, registry_package
    );
    if !are_cargo_toml_equal(local_package, registry_package) {
        debug!("Cargo.toml is different");
        return Ok(false);
    }

    let local_package_files = get_cargo_package_files(local_package).with_context(|| {
        format!("cannot determine packaged files of local package {local_package:?}")
    })?;
    let registry_package_files = registry_package_files.get(registry_package)?;

    // Older published libraries may lack Cargo.lock, but modern `cargo package --list`
    // includes it even when absent. Ignore its presence to preserve the comparison
    // behavior from when both sides used Cargo's file list. Its contents can also
    // differ in workspaces; the updater separately checks dependency versions for
    // executables when both lockfiles exist.
    let is_comparable_file = |file: &&Utf8PathBuf| {
        !matches!(
            file.as_str(),
            "Cargo.toml.orig" | ".cargo_vcs_info.json" | "Cargo.lock"
        )
    };
    let local_files = local_package_files.iter().filter(is_comparable_file);

    let registry_files = registry_package_files
        .iter()
        .filter(is_comparable_file)
        // Cargo creates this marker when extracting a registry package.
        .filter(|file| *file != ".cargo-ok");

    if !local_files.clone().eq(registry_files) {
        // New files were added or removed.
        debug!("cargo package list is different");
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

pub fn get_cargo_package_files(package: &Utf8Path) -> anyhow::Result<Vec<Utf8PathBuf>> {
    // Cargo stores the original manifest in `Cargo.toml.orig` when packaging a crate.
    // Downloaded and locally unpacked crates already contain the packaged files.
    debug!("Getting packaged files for crate at {}", package);
    if package.join("Cargo.toml.orig").is_file() {
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
    // Local packages can contain uncommitted changes during an update.
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
            // Git metadata isn't part of the package, including nested repositories
            // and worktrees whose `.git` entry is a file.
            if path.file_name() == Some(".git") {
                continue;
            }
            let file_type = entry
                .file_type()
                .with_context(|| format!("cannot read file type for {path:?}"))?;

            if file_type.is_dir() {
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
    let original_manifest = registry_package.join("Cargo.toml.orig");
    let released_manifest = if original_manifest.is_file() {
        original_manifest
    } else {
        // Git-only releases retain their original workspace manifests.
        registry_package.join(CARGO_TOML)
    };
    are_files_equal(&local_package.join(CARGO_TOML), &released_manifest).unwrap_or(false)
}

/// Returns true if the README file of the local package is the same as the one in the registry.
/// Returns false if:
/// - the README is the same
/// - the local package doesn't have a `readme` field in the `Cargo.toml`.
/// - the package doesn't have a README at all.
pub(crate) fn is_readme_updated(
    package_name: &str,
    local_package_path: &Utf8Path,
    registry_package_path: &Utf8Path,
    released_package: &Package,
) -> anyhow::Result<bool> {
    // Read again manifest metadata because the Cargo.toml might change on every commit.
    let package = match read_package_metadata(&local_package_path.join(CARGO_TOML), package_name) {
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
            let registry_package_readme_path = if registry_package_path
                .join("Cargo.toml.orig")
                .is_file()
            {
                registry_package_path.join("README.md")
            } else {
                // A released package that was never packaged keeps its original manifest,
                // so its `readme` field can point outside the package directory.
                let Some(path) = local_readme_override(released_package, registry_package_path)?
                else {
                    return Ok(true);
                };
                path
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_utils::Utf8TempDir;

    #[test]
    fn unpacked_package_is_listed_without_running_cargo() {
        let package = Utf8TempDir::new().unwrap();
        let package = package.path();
        fs_err::create_dir_all(package.join("src/nested")).unwrap();
        fs_err::create_dir(package.join(".git")).unwrap();
        fs_err::create_dir_all(package.join("src/nested/.git/objects")).unwrap();
        for file in [
            "Cargo.toml",
            "Cargo.toml.orig",
            ".hidden",
            "src/nested/lib.rs",
            "src/nested.rs",
            ".git/config",
            "src/nested/.git/objects/metadata",
            "src/.git",
        ] {
            // In particular, Cargo.toml is invalid, so invoking Cargo would fail.
            fs_err::write(package.join(file), "packaged content").unwrap();
        }

        assert_eq!(
            get_cargo_package_files(package).unwrap(),
            [
                ".hidden",
                "Cargo.toml",
                "Cargo.toml.orig",
                "src/nested.rs",
                "src/nested/lib.rs",
            ]
            .map(Utf8PathBuf::from)
        );
    }

    #[test]
    fn package_without_original_manifest_uses_cargo_file_selection() {
        let package = test_package();
        fs_err::write(package.path().join("another_file"), "file").unwrap();
        let files = get_cargo_package_files(package.path()).unwrap();

        assert!(files.contains(&Utf8PathBuf::from("src/lib.rs")));
        assert!(files.contains(&Utf8PathBuf::from("another_file")));
        assert!(!files.contains(&Utf8PathBuf::from("excluded.txt")));
        assert!(!package.path().join("Cargo.toml.orig").exists());
    }

    #[test]
    fn compare_downloaded_package_ignores_git_metadata_and_detects_changes() {
        let local = test_package();
        let repo = git_cmd::Repo::init(local.path());
        fs_err::write(local.path().join(".hidden"), "hidden content").unwrap();
        repo.add_all_and_commit("initial package").unwrap();
        let output = run_cargo(local.path(), &["package", "--no-verify", "--quiet"]).unwrap();
        assert!(output.status.success(), "{}", output.stderr);

        let registry = Utf8TempDir::new().unwrap();
        let archive =
            fs_err::File::open(local.path().join("target/package/example-0.1.0.crate")).unwrap();
        tar::Archive::new(flate2::read::GzDecoder::new(archive))
            .unpack(registry.path())
            .unwrap();
        let registry = registry.path().join("example-0.1.0");
        // Downloaded packages are initialized as Git repositories by release-plz.
        git_cmd::Repo::init(&registry);

        assert!(are_packages_equal(local.path(), &registry).unwrap());
        assert!(registry.join("Cargo.toml.orig").is_file());

        // Exercise registry metadata while the local side still uses Cargo's file list.
        fs_err::write(registry.join(".cargo-ok"), "{}").unwrap();
        fs_err::write(registry.join("Cargo.lock"), "historical lockfile").unwrap();
        fs_err::remove_dir_all(registry.join(".git")).unwrap();
        fs_err::write(registry.join(".git"), "gitdir: /elsewhere/worktrees/crate").unwrap();
        assert!(are_packages_equal(local.path(), &registry).unwrap());

        // Libraries published before Cargo 1.84 may not contain a lockfile, even
        // though modern `cargo package --list` includes it on the local side.
        fs_err::remove_file(registry.join("Cargo.lock")).unwrap();
        assert!(are_packages_equal(local.path(), &registry).unwrap());

        fs_err::write(registry.join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        assert!(!are_packages_equal(local.path(), &registry).unwrap());
        fs_err::copy(local.path().join("src/lib.rs"), registry.join("src/lib.rs")).unwrap();

        fs_err::write(registry.join("extra.txt"), "added file").unwrap();
        assert!(!are_packages_equal(local.path(), &registry).unwrap());
        fs_err::remove_file(registry.join("extra.txt")).unwrap();
        fs_err::remove_file(registry.join(".hidden")).unwrap();
        assert!(!are_packages_equal(local.path(), &registry).unwrap());
    }

    #[test]
    fn compare_packaged_files_ignores_extraction_marker() {
        let local = test_package();
        let registry = test_package();
        // Use the disk-listing path on both sides to isolate comparison filtering.
        for package in [&local, &registry] {
            fs_err::copy(
                package.path().join(CARGO_TOML),
                package.path().join("Cargo.toml.orig"),
            )
            .unwrap();
        }
        fs_err::write(registry.path().join(".cargo-ok"), "{}").unwrap();

        assert!(are_packages_equal(local.path(), registry.path()).unwrap());

        // A similarly named file inside the package is still compared.
        fs_err::write(registry.path().join("src/.cargo-ok"), "content").unwrap();
        assert!(!are_packages_equal(local.path(), registry.path()).unwrap());
    }

    #[test]
    fn compare_packaged_files_ignores_lockfile_presence_and_contents() {
        let local = test_package();
        let registry = test_package();
        for package in [&local, &registry] {
            fs_err::copy(
                package.path().join(CARGO_TOML),
                package.path().join("Cargo.toml.orig"),
            )
            .unwrap();
        }

        fs_err::write(local.path().join("Cargo.lock"), "local lockfile").unwrap();
        assert!(are_packages_equal(local.path(), registry.path()).unwrap());

        fs_err::write(registry.path().join("Cargo.lock"), "published lockfile").unwrap();
        assert!(are_packages_equal(local.path(), registry.path()).unwrap());

        fs_err::remove_file(local.path().join("Cargo.lock")).unwrap();
        assert!(are_packages_equal(local.path(), registry.path()).unwrap());

        // Nested lockfiles remain part of the packaged file list.
        fs_err::write(registry.path().join("src/Cargo.lock"), "nested lockfile").unwrap();
        assert!(!are_packages_equal(local.path(), registry.path()).unwrap());
    }

    #[test]
    fn compare_source_packages_uses_cargo_file_selection() {
        let local = test_package();
        let released = test_package();
        assert!(are_packages_equal(local.path(), released.path()).unwrap());

        // Source packages have no Cargo.toml.orig; manifest-only changes still count.
        let manifest_path = released.path().join(CARGO_TOML);
        let original_manifest = fs_err::read_to_string(&manifest_path).unwrap();
        fs_err::write(
            &manifest_path,
            format!("{original_manifest}description = \"Updated description\"\n"),
        )
        .unwrap();
        assert!(!are_packages_equal(local.path(), released.path()).unwrap());
        fs_err::write(&manifest_path, original_manifest).unwrap();
        assert!(are_packages_equal(local.path(), released.path()).unwrap());

        fs_err::write(released.path().join("excluded.txt"), "ignored change").unwrap();
        assert!(are_packages_equal(local.path(), released.path()).unwrap());
        fs_err::write(released.path().join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        assert!(!are_packages_equal(local.path(), released.path()).unwrap());
        fs_err::copy(
            local.path().join("src/lib.rs"),
            released.path().join("src/lib.rs"),
        )
        .unwrap();
        fs_err::write(released.path().join("extra.txt"), "new file").unwrap();
        assert!(!are_packages_equal(local.path(), released.path()).unwrap());
        fs_err::remove_file(released.path().join("extra.txt")).unwrap();
        fs_err::remove_file(released.path().join("src/lib.rs")).unwrap();
        // Keep the target valid while testing a removed packaged file.
        fs_err::write(released.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        assert!(!are_packages_equal(local.path(), released.path()).unwrap());
    }

    fn test_package() -> Utf8TempDir {
        let package = Utf8TempDir::new().unwrap();
        fs_err::create_dir(package.path().join("src")).unwrap();
        fs_err::write(
            package.path().join("Cargo.toml"),
            r#"[package]
name = "example"
version = "0.1.0"
edition = "2024"
exclude = ["excluded.txt"]
"#,
        )
        .unwrap();
        fs_err::write(package.path().join("src/lib.rs"), "pub fn example() {}\n").unwrap();
        fs_err::write(package.path().join("excluded.txt"), "not packaged").unwrap();
        package
    }
}
