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

    // Source-vs-source fallback: when cargo package failed in the git_only flow
    // (see `next_ver::get_cargo_package_from_source`), `registry_package` points
    // to a worktree source directory rather than a `target/package/<name>-<v>`
    // tarball. In that case there's no `Cargo.toml.orig` and we cannot use the
    // cargo-package-list path; compare directly from source using a filesystem
    // walk that respects git-tracked files.
    //
    // Tracks: <https://github.com/release-plz/release-plz/issues/2595>
    let registry_is_source_dir = !registry_package.join("Cargo.toml.orig").exists();
    if registry_is_source_dir {
        return are_source_dirs_equal(local_package, registry_package);
    }

    if !are_cargo_toml_equal(local_package, registry_package) {
        debug!("Cargo.toml is different");
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

    let local_files = local_package_files
        .iter()
        .filter(|file| *file != "Cargo.toml.orig" && *file != ".cargo_vcs_info.json");

    let registry_files = registry_package_files.iter().filter(|file| {
        *file != "Cargo.toml.orig"
            && *file != "Cargo.toml.orig.orig"
            && *file != ".cargo_vcs_info.json"
    });

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

fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> anyhow::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs_err::rename(from, to).with_context(|| format!("cannot rename {from:?} to {to:?}"))
}

/// Compare two worktree source directories file-by-file.
///
/// Used as a fallback when `cargo package` fails for the baseline commit in
/// `git_only` mode (see `next_ver::process_git_only_package`). Because the
/// baseline has no packaged tarball we cannot rely on the `cargo package --list`
/// path; instead we list files via `git ls-files` in each source directory and
/// hash them, skipping any file not tracked by git.
///
/// This is a more conservative comparison than the packaged-tarball one:
/// - Source files that would have been excluded by `Cargo.toml` `exclude` / `include`
///   rules (e.g. `target/`, hidden dev helpers) are skipped because they are
///   normally not tracked by git anyway.
/// - Files listed by `.gitignore` are excluded.
/// - Generated files (`Cargo.lock`, `Cargo.toml.orig`) are ignored.
///
/// Tracks: <https://github.com/release-plz/release-plz/issues/2595>
fn are_source_dirs_equal(
    local_package: &Utf8Path,
    registry_package: &Utf8Path,
) -> anyhow::Result<bool> {
    // Compare `Cargo.toml` verbatim — this is the source manifest on both sides
    // (not a `Cargo.toml.orig` tarball artifact).
    if !are_files_equal(
        &local_package.join(CARGO_TOML),
        &registry_package.join(CARGO_TOML),
    )
    .unwrap_or(false)
    {
        debug!("source Cargo.toml is different");
        return Ok(false);
    }

    let local_files = list_git_tracked_files(local_package).with_context(|| {
        format!("cannot list git-tracked files in local source at {local_package:?}")
    })?;
    let registry_files = list_git_tracked_files(registry_package).with_context(|| {
        format!("cannot list git-tracked files in registry source at {registry_package:?}")
    })?;

    let local_set: std::collections::BTreeSet<&Utf8PathBuf> = local_files
        .iter()
        .filter(|f| is_comparable_source_file(f))
        .collect();
    let registry_set: std::collections::BTreeSet<&Utf8PathBuf> = registry_files
        .iter()
        .filter(|f| is_comparable_source_file(f))
        .collect();

    if local_set != registry_set {
        debug!(
            "source file lists differ: local has {} files, registry has {}",
            local_set.len(),
            registry_set.len()
        );
        return Ok(false);
    }

    for rel_path in local_set {
        let local_path = local_package.join(rel_path);
        let registry_path = registry_package.join(rel_path);
        if !local_path.exists() || !registry_path.exists() {
            continue;
        }
        if local_path.is_symlink() || registry_path.is_symlink() {
            continue;
        }
        if !are_files_equal(&local_path, &registry_path)
            .with_context(|| format!("cannot compare {local_path:?} vs {registry_path:?}"))?
        {
            debug!("source file {rel_path} differs");
            return Ok(false);
        }
    }

    Ok(true)
}

/// Run `git ls-files` in `package` and return the tracked file paths relative
/// to that directory. Paths are sorted for deterministic comparison.
fn list_git_tracked_files(package: &Utf8Path) -> anyhow::Result<Vec<Utf8PathBuf>> {
    let output = std::process::Command::new("git")
        .current_dir(package.as_std_path())
        .args(["ls-files", "-z"])
        .output()
        .with_context(|| format!("cannot run git ls-files in {package:?}"))?;

    anyhow::ensure!(
        output.status.success(),
        "git ls-files failed in {:?}: {}",
        package,
        String::from_utf8_lossy(&output.stderr)
    );

    let mut files: Vec<Utf8PathBuf> = output
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|bytes| std::str::from_utf8(bytes).ok())
        .map(Utf8PathBuf::from)
        .collect();
    files.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(files)
}

/// Files that should be skipped when comparing source directories.
///
/// We filter out build artifacts and auto-generated files that would otherwise
/// cause spurious differences between two source trees.
fn is_comparable_source_file(path: &Utf8Path) -> bool {
    !matches!(
        path.file_name(),
        Some("Cargo.lock")
            | Some("Cargo.toml.orig")
            | Some("Cargo.toml.orig.orig")
            | Some(".cargo_vcs_info.json")
    )
}

pub fn get_cargo_package_files(package: &Utf8Path) -> anyhow::Result<Vec<Utf8PathBuf>> {
    // If this crate was packaged locally (i.e. is inside target/package), we can list files
    // directly from disk without invoking `cargo package`.
    // At the moment, this only happens in the git_only flow.
    // TODO: Do this always, not only if we are in target/package.
    //       See https://github.com/release-plz/release-plz/issues/2130
    info!("Getting packaged files for crate at {}", package);
    if is_cargo_packaged_dir(package)
        && (package.join("Cargo.toml.orig").exists()
            || package.join("Cargo.toml.orig.orig").exists())
    {
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

fn is_cargo_packaged_dir(package: &Utf8Path) -> bool {
    package.ancestors().any(|ancestor| {
        ancestor.file_name() == Some("package")
            && ancestor.parent().and_then(|parent| parent.file_name()) == Some("target")
    })
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
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a throwaway git repo under `root` containing two identical source
    /// package directories `a/` and `b/`. Both have a minimal `Cargo.toml` plus
    /// an optional additional file, mimicking worktree source trees that might
    /// be compared when `cargo package` failed in `git_only` mode.
    fn init_repo_with_two_source_packages(
        root: &Utf8Path,
        extra_file: Option<(&str, &str)>,
    ) -> anyhow::Result<()> {
        Command::new("git")
            .current_dir(root)
            .args(["init", "-q"])
            .status()?;
        Command::new("git")
            .current_dir(root)
            .args(["config", "user.email", "test@example.com"])
            .status()?;
        Command::new("git")
            .current_dir(root)
            .args(["config", "user.name", "test"])
            .status()?;

        let cargo_toml = r#"[package]
name = "demo"
version = "0.1.0"
edition = "2021"
"#;
        for dir in ["a", "b"] {
            let pkg = root.join(dir);
            fs_err::create_dir_all(&pkg)?;
            fs_err::write(pkg.join("Cargo.toml"), cargo_toml)?;
            if let Some((name, contents)) = extra_file {
                fs_err::write(pkg.join(name), contents)?;
            }
        }
        Command::new("git")
            .current_dir(root)
            .args(["add", "-A"])
            .status()?;
        Command::new("git")
            .current_dir(root)
            .args(["commit", "-qm", "init"])
            .status()?;
        Ok(())
    }

    #[test]
    fn source_dir_comparison_identical_sources_are_equal() {
        let tmp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        init_repo_with_two_source_packages(&root, Some(("lib.rs", "fn a() {}\n")))
            .expect("init repo");

        let result =
            are_packages_equal(&root.join("a"), &root.join("b")).expect("are_packages_equal");
        assert!(
            result,
            "two source directories with identical content must compare equal"
        );
    }

    #[test]
    fn source_dir_comparison_different_content_is_not_equal() {
        let tmp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        init_repo_with_two_source_packages(&root, Some(("lib.rs", "fn a() {}\n")))
            .expect("init repo");

        // Make `a/lib.rs` differ AFTER initial commit; git ls-files still reports
        // the tracked file, but content hash will differ.
        fs_err::write(root.join("a").join("lib.rs"), "fn different() {}\n").unwrap();

        let result =
            are_packages_equal(&root.join("a"), &root.join("b")).expect("are_packages_equal");
        assert!(
            !result,
            "source directories with differing file content must compare NOT equal"
        );
    }

    #[test]
    fn source_dir_comparison_different_cargo_toml_is_not_equal() {
        let tmp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        init_repo_with_two_source_packages(&root, None).expect("init repo");

        // Change `a/Cargo.toml`.
        fs_err::write(
            root.join("a").join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.2.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let result =
            are_packages_equal(&root.join("a"), &root.join("b")).expect("are_packages_equal");
        assert!(
            !result,
            "source directories with differing Cargo.toml must compare NOT equal"
        );
    }

    #[test]
    fn source_dir_comparison_ignores_cargo_lock_and_vcs_info() {
        let tmp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        init_repo_with_two_source_packages(&root, Some(("lib.rs", "fn a() {}\n")))
            .expect("init repo");

        // Create untracked Cargo.lock and .cargo_vcs_info.json in `a`.
        // These files are filtered by `is_comparable_source_file` and must not
        // affect the comparison. `git ls-files` also won't return them since
        // they're untracked.
        fs_err::write(root.join("a").join("Cargo.lock"), "# lock\n").unwrap();
        fs_err::write(
            root.join("a").join(".cargo_vcs_info.json"),
            "{\"git\": {\"sha1\": \"deadbeef\"}}",
        )
        .unwrap();

        let result =
            are_packages_equal(&root.join("a"), &root.join("b")).expect("are_packages_equal");
        assert!(
            result,
            "Cargo.lock and .cargo_vcs_info.json must be ignored in source comparison"
        );
    }

    #[test]
    fn is_comparable_source_file_filters_known_artifacts() {
        assert!(!is_comparable_source_file(Utf8Path::new("Cargo.lock")));
        assert!(!is_comparable_source_file(Utf8Path::new("Cargo.toml.orig")));
        assert!(!is_comparable_source_file(Utf8Path::new(
            "Cargo.toml.orig.orig"
        )));
        assert!(!is_comparable_source_file(Utf8Path::new(
            ".cargo_vcs_info.json"
        )));
        assert!(is_comparable_source_file(Utf8Path::new("src/lib.rs")));
        assert!(is_comparable_source_file(Utf8Path::new("Cargo.toml")));
        assert!(is_comparable_source_file(Utf8Path::new("README.md")));
    }
}
