// Copied from [cargo-clone](https://github.com/JanLikar/cargo-clone/blob/89ba4da215663ffb3b8c93a674f3002937eafec4/cargo-clone-core/src/lib.rs)
//! Fetch the source code of a Rust crate from a registry.

#![warn(missing_docs)]

mod cloner_builder;
mod source;

use cargo::util::cache_lock::CacheLockMode;
use cargo_metadata::camino::Utf8Path;
use cargo_metadata::camino::Utf8PathBuf;
pub use cloner_builder::*;
pub use source::*;
use tracing::warn;

use std::collections::HashSet;

use std::process::Command;

use anyhow::{Context, bail};

use cargo::core::Package;
use cargo::core::dependency::Dependency;
use cargo::sources::source::{QueryKind, Source};
use cargo::sources::{IndexSummary, PathSource, SourceConfigMap};

use walkdir::WalkDir;

// Re-export cargo types.
pub use cargo::{
    core::SourceId,
    util::{CargoResult, GlobalContext},
};

use crate::fs_utils::strip_prefix;
use crate::fs_utils::to_utf8_path;

/// Rust crate.
#[derive(PartialEq, Eq, Debug)]
pub struct Crate {
    name: String,
    version: Option<String>,
}

impl Crate {
    /// Create a new [`Crate`].
    /// If `version` is not specified, the latest version is chosen.
    pub fn new(name: String, version: Option<String>) -> Crate {
        Crate { name, version }
    }
}

/// Clones a crate.
pub struct Cloner {
    /// Cargo configuration.
    pub(crate) config: GlobalContext,
    /// Directory where the crates will be cloned.
    /// Each crate is cloned into a subdirectory of this directory.
    pub(crate) directory: Utf8PathBuf,
    /// Where the crates will be cloned from.
    pub(crate) srcid: SourceId,
    /// If true, use `git` to clone the git repository present in the manifest metadata.
    pub(crate) use_git: bool,
}

impl Cloner {
    /// Creates a new [`ClonerBuilder`] that:
    /// - Uses crates.io as source.
    /// - Clones the crates into the current directory.
    pub fn builder() -> ClonerBuilder {
        ClonerBuilder::new()
    }

    #[allow(dead_code)]
    /// Queries info of the specified crate without downloading it.
    pub fn query_latest_package(&self, name: &str) -> CargoResult<Option<IndexSummary>> {
        let _lock = self.acquire_cargo_package_cache_lock()?;
        let mut src = self.get_source()?;
        query_latest_package_summary(&mut src, name, None)
    }

    fn clone_from_summary_into(
        &self,
        summary: &IndexSummary,
        dest_path: &Utf8Path,
        src: &mut impl Source,
    ) -> CargoResult<Package> {
        let name = summary.as_summary().name();

        self.config.shell().note(format!(
            "Downloading {} {}",
            name,
            summary.as_summary().version()
        ))?;
        let pkg = Box::new(src).download_now(summary.package_id(), &self.config)?;

        if self.use_git {
            let repo = pkg
                .manifest()
                .metadata()
                .repository
                .as_ref()
                .with_context(|| {
                    format!(
                        "Cannot clone {} from git repo because \
                        repository is not specified in package's manifest.",
                        &name
                    )
                })?;

            clone_git_repo(repo, dest_path)?;
        } else {
            clone_directory(to_utf8_path(pkg.root())?, dest_path)
                .context("failed to clone directory")?;
        }

        Ok(pkg)
    }

    /// Clone the specified crates from registry or git repository.
    /// Each crate is cloned in a subdirectory named as the crate name.
    /// Returns the cloned crates and the path where they are cloned.
    /// If a crate doesn't exist, is not returned.
    pub fn clone(&self, crates: &[Crate]) -> CargoResult<Vec<(Package, Utf8PathBuf)>> {
        let _lock = self.acquire_cargo_package_cache_lock()?;
        let mut src = self.get_source()?;
        let mut cloned_pkgs = vec![];

        for crate_ in crates {
            let mut dest_path = self.directory.clone();

            dest_path.push(&crate_.name);

            let pkg = self
                .clone_in(crate_, &dest_path, &mut src)
                .with_context(|| {
                    format!("failed to clone package {} in {dest_path}", &crate_.name)
                })?;

            if let Some(pkg) = pkg {
                cloned_pkgs.push((pkg, dest_path));
            }
        }

        Ok(cloned_pkgs)
    }

    fn acquire_cargo_package_cache_lock(&self) -> CargoResult<cargo::util::cache_lock::CacheLock> {
        self.config
            .acquire_package_cache_lock(CacheLockMode::DownloadExclusive)
    }

    fn get_source(&self) -> CargoResult<Box<dyn Source + '_>> {
        let mut source = if self.srcid.is_path() {
            let path = self.srcid.url().to_file_path().expect("path must be valid");
            Box::new(PathSource::new(&path, self.srcid, &self.config))
        } else {
            let map = SourceConfigMap::new(&self.config)?;
            map.load(self.srcid, &HashSet::default())?
        };

        source.invalidate_cache();
        Ok(source)
    }

    fn clone_in(
        &self,
        crate_: &Crate,
        dest_path: &Utf8Path,
        src: &mut impl Source,
    ) -> CargoResult<Option<Package>> {
        if !dest_path.exists() {
            fs_err::create_dir_all(dest_path)?;
        }

        self.config
            .shell()
            .verbose(|s| s.note(format!("Cloning into {:?}", &self.directory)))?;

        // Cloning into an existing directory is only allowed if the directory is empty.
        let is_empty = dest_path.read_dir()?.next().is_none();
        if !is_empty {
            bail!(
                "destination path '{}' already exists and is not an empty directory.",
                dest_path
            );
        }

        self.clone_single(crate_, dest_path, src)
    }

    /// Clone one crate.
    fn clone_single(
        &self,
        crate_: &Crate,
        dest_path: &Utf8Path,
        src: &mut impl Source,
    ) -> CargoResult<Option<Package>> {
        let name = &crate_.name;
        let vers = crate_.version.as_deref();
        let latest = query_latest_package_summary(src, name, vers)?;

        let pkg = match latest {
            Some(l) => {
                let pkg = self.clone_from_summary_into(&l, dest_path, src)?;
                Some(pkg)
            }
            None => {
                warn!("Package `{}@{}` not found", name, vers.unwrap_or("*.*.*"));
                None
            }
        };
        Ok(pkg)
    }
}

fn query_latest_package_summary(
    src: &mut impl Source,
    name: &str,
    vers: Option<&str>,
) -> CargoResult<Option<IndexSummary>> {
    let dep = Dependency::parse(name, vers, src.source_id())?;
    let mut latest_summary: Option<IndexSummary> = None;
    loop {
        let query_result = src.query(&dep, QueryKind::Exact, &mut |summary| {
            let is_summary_newer = latest_summary.as_ref().is_none_or(|latest| {
                latest.as_summary().version() < summary.as_summary().version()
            });
            if is_summary_newer {
                latest_summary = Some(summary);
            };
        });
        match query_result {
            std::task::Poll::Ready(res) => match res {
                Ok(()) => break,
                Err(err) => {
                    return none_or_query_err(err);
                }
            },
            std::task::Poll::Pending => match src.block_until_ready() {
                Ok(()) => {}
                Err(err) => {
                    return none_or_query_err(err);
                }
            },
        }
    }
    Ok(latest_summary)
}

fn none_or_query_err<T>(err: anyhow::Error) -> CargoResult<Option<T>> {
    if err.to_string().contains("failed to fetch") {
        // I observed this error happens when the cargo registry contains no crates.
        // If this isn't the case, open an issue.
        warn!("Failed to fetch package from registry. I assume the registry is empty.");
        Ok(None)
    } else {
        Err(err)
    }
}

// clone_directory copies the contents of one directory into another directory, which must
// already exist.
fn clone_directory(from: &Utf8Path, to: &Utf8Path) -> CargoResult<()> {
    if !to.is_dir() {
        bail!("Not a directory: {to}");
    }
    for entry in WalkDir::new(from) {
        let entry = entry.unwrap();
        let file_type = entry.file_type();
        let mut dest_path = to.to_owned();
        let utf8_entry: &Utf8Path = entry.path().try_into()?;
        dest_path.push(strip_prefix(utf8_entry, from).unwrap());

        if entry.file_name() == ".cargo-ok" {
            continue;
        }

        if !file_type.is_dir() {
            // .cargo-ok is not wanted in this context
            fs_err::copy(entry.path(), &dest_path)?;
        } else if file_type.is_dir() {
            if dest_path == to {
                continue;
            }
            fs_err::create_dir(&dest_path)?;
        }
    }

    Ok(())
}

fn clone_git_repo(repo: &str, to: &Utf8Path) -> CargoResult<()> {
    let status = Command::new("git")
        .arg("clone")
        .arg(repo)
        .arg(to)
        .status()
        .context("Failed to clone from git repo.")?;

    if !status.success() {
        bail!("Failed to clone from git repo.")
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use fake::Fake;
    use tempfile::tempdir;

    use super::*;

    #[test]
    #[ignore = "requires network"]
    fn query_latest_package_works_for_existing() {
        let package_name = "rand";
        let temp_dir = tempdir().unwrap();
        let directory = temp_dir.as_ref().to_str().expect("invalid tempdir path");
        let cloner = ClonerBuilder::new()
            .with_directory(directory)
            .build()
            .unwrap();

        let rand = cloner
            .query_latest_package(package_name)
            .unwrap()
            .unwrap()
            .into_summary();
        assert_eq!(rand.name(), package_name);

        // Generate random string 15 characters long.
        let package: String = 15.fake();
        assert!(cloner.query_latest_package(&package).unwrap().is_none());
    }
}
