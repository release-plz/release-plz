//! A deliberately narrow fast path for listing historical package snapshots.
//!
//! Cargo does not inspect Rust source contents to select packaged files. In a
//! clean, entirely tracked checkout, changing only those contents can therefore
//! reuse a listing. Everything else (including unsupported Git configurations)
//! falls back to asking Cargo. External dependencies and configuration are
//! assumed stable during a history walk, just as for the released-package cache.

use anyhow::Context as _;
use cargo_metadata::camino::Utf8Path;
use std::{io::Read as _, path::Path};

use super::PackageFiles;

/// Keep the optimization bounded even in repositories with huge ignored trees
/// or assets. Exceeding these limits only disables reuse, never the update.
const MAX_ENTRIES: usize = 10_000;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
pub(crate) struct HistoryPackageFiles {
    snapshot: Option<SelectionInputs>,
    files: PackageFiles,
}

impl HistoryPackageFiles {
    /// Must be called after each checkout, before inspecting package contents.
    pub(crate) fn at_current_commit(
        &mut self,
        repository: &Utf8Path,
        package: &Utf8Path,
    ) -> &PackageFiles {
        let snapshot = SelectionInputs::read(repository, package)
            .inspect_err(|error| tracing::debug!("package file-list cache unavailable: {error:#}"))
            .ok();
        if snapshot.is_none() || snapshot != self.snapshot {
            self.files = PackageFiles::default();
        } else if self.files.is_cached() {
            tracing::debug!("reusing historical package file list for {package}");
        }
        self.snapshot = snapshot;
        &self.files
    }
}

#[derive(PartialEq, Eq)]
struct SelectionInputs {
    entries: Vec<(Vec<u8>, u32, Option<git2::Oid>)>,
    git_config: Vec<(Vec<u8>, Vec<u8>)>,
    cargo_config: Vec<Option<Vec<u8>>>,
    git_exclude: Option<Vec<u8>>,
}

impl SelectionInputs {
    fn read(root: &Utf8Path, package: &Utf8Path) -> anyhow::Result<Self> {
        for name in ["GIT_CONFIG_COUNT", "GIT_CONFIG_PARAMETERS"] {
            anyhow::ensure!(
                std::env::var_os(name).is_none(),
                "Git configuration overrides"
            );
        }
        let repo = git2::Repository::open(root)?;
        // Linked worktrees and submodules can have selection inputs elsewhere.
        let git_dir = root.join(".git");
        anyhow::ensure!(
            fs_err::symlink_metadata(&git_dir)?.is_dir()
                && fs_err::canonicalize(repo.path())? == fs_err::canonicalize(&git_dir)?,
            "nonlocal Git directory"
        );
        let index = repo.index()?;
        anyhow::ensure!(index.len() <= MAX_ENTRIES, "large Git index");
        anyhow::ensure!(!index.has_conflicts(), "conflicted Git index");
        let mut entries = Vec::with_capacity(index.len());
        for entry in index.iter() {
            anyhow::ensure!(
                matches!(entry.mode, 0o100_644 | 0o100_755),
                "symlink or submodule in Git index"
            );
            // assume-valid, skip-worktree, and intent-to-add can hide changes
            // from status; do not trust their index entries as disk contents.
            anyhow::ensure!(
                entry.flags & 0x8000 == 0 && entry.flags_extended == 0,
                "special Git index flags"
            );
            // Executable scripts can be selected as Cargo/Rustc wrappers.
            let source = entry.mode == 0o100_644 && entry.path.ends_with(b".rs");
            entries.push((entry.path, entry.mode, (!source).then_some(entry.id)));
        }

        let config = repo.config()?;
        let mut git_config = Vec::new();
        let mut has_filters = false;
        let mut config_entries = config.entries(None)?;
        while let Some(entry) = config_entries.next() {
            let entry = entry?;
            let name = entry.name()?;
            // Arbitrarily named source files can be Git config/ignore inputs
            // or filter programs. Filters can also make status report clean
            // while on-disk control files differ from their indexed blobs.
            anyhow::ensure!(
                !name.starts_with("include.")
                    && !name.starts_with("includeif.")
                    && name != "core.excludesfile"
                    && name != "core.attributesfile"
                    && name != "core.fsmonitor",
                "indirect Git configuration"
            );
            has_filters |= name.starts_with("filter.");
            git_config.push((entry.name_bytes().to_vec(), entry.value_bytes().to_vec()));
        }
        if has_filters {
            for entry in index.iter() {
                let path = Path::new(std::str::from_utf8(&entry.path)?);
                let filter = repo.get_attr(path, "filter", git2::AttrCheckFlags::default())?;
                anyhow::ensure!(
                    matches!(
                        git2::AttrValue::from_string(filter),
                        git2::AttrValue::Unspecified | git2::AttrValue::False
                    ),
                    "Git content filter"
                );
            }
        }

        // Check disk membership as well as index membership. In particular,
        // nested repositories, symlinks, and ignored/untracked files can change
        // Cargo's traversal without changing the index. Bound this walk before
        // asking libgit2 for status, which otherwise can scan huge ignored trees.
        let walker = walkdir::WalkDir::new(root)
            .into_iter()
            .filter_entry(|entry| entry.depth() != 1 || entry.file_name() != ".git");
        for (count, entry) in walker.enumerate() {
            anyhow::ensure!(count < MAX_ENTRIES, "large working tree");
            let entry = entry?;
            anyhow::ensure!(entry.file_name() != ".git", "nested Git repository");
            let kind = entry.file_type();
            #[expect(
                clippy::filetype_is_file,
                reason = "Symlinks and special files must disable the cache"
            )]
            let regular_file = kind.is_file();
            anyhow::ensure!(
                kind.is_dir() || regular_file,
                "nonregular working-tree file"
            );
            if regular_file {
                let path = entry.path().strip_prefix(root)?;
                anyhow::ensure!(index.get_path(path, 0).is_some(), "untracked file");
            }
        }
        let mut options = git2::StatusOptions::new();
        options.include_untracked(true).include_ignored(true);
        anyhow::ensure!(
            repo.statuses(Some(&mut options))?.is_empty(),
            "dirty checkout"
        );

        let mut cargo_config = Vec::new();
        for ancestor in package.ancestors() {
            read_cargo_configs(ancestor.join(".cargo").as_std_path(), &mut cargo_config)?;
        }
        let cargo_home =
            cargo::util::homedir(package.as_std_path()).context("cannot determine Cargo home")?;
        read_cargo_configs(&cargo_home, &mut cargo_config)?;

        Ok(Self {
            entries,
            git_config,
            cargo_config,
            git_exclude: read_config(&repo.path().join("info/exclude"))?,
        })
    }
}

fn read_cargo_configs(directory: &Path, configs: &mut Vec<Option<Vec<u8>>>) -> anyhow::Result<()> {
    for name in ["config", "config.toml"] {
        let contents = read_config(&directory.join(name))?;
        if let Some(contents) = &contents {
            let config: toml::Table = std::str::from_utf8(contents)?.parse()?;
            // Cargo's unstable config includes can point at any filename,
            // including a Rust source file whose blob ID we otherwise omit.
            anyhow::ensure!(!config.contains_key("include"), "Cargo config includes");
        }
        configs.push(contents);
    }
    Ok(())
}

fn read_config(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let file = match fs_err::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut contents = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut contents)?;
    anyhow::ensure!(
        contents.len() as u64 <= MAX_CONFIG_BYTES,
        "large configuration file"
    );
    Ok(Some(contents))
}
