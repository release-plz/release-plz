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

use std::process::Command;

use anyhow::{Context, bail};

use cargo::sources::source::{QueryKind, Source, SourceMap};
use cargo::sources::{IndexSummary, PathSource, SourceConfigMap};
use cargo::workspace::{Dependency, Package, PackageSet};

use walkdir::WalkDir;

// Re-export cargo types.
pub use cargo::{
    util::{CargoResult, GlobalContext},
    workspace::SourceId,
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
    pub fn new(name: String, version: Option<String>) -> Self {
        Self { name, version }
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

    fn clone_package_into(&self, pkg: &Package, dest_path: &Utf8Path) -> CargoResult<()> {
        // Check again after downloading: an earlier request may have used this destination.
        prepare_destination(dest_path)?;
        let name = pkg.name();

        if self.use_git {
            let repo = pkg
                .manifest()
                .metadata()
                .repository
                .as_ref()
                .with_context(|| {
                    format!(
                        "Cannot clone {name} from git repo because \
                        repository is not specified in package's manifest."
                    )
                })?;

            clone_git_repo(repo, dest_path)?;
        } else {
            clone_directory(to_utf8_path(pkg.root())?, dest_path)
                .context("failed to clone directory")?;
        }

        Ok(())
    }

    /// Clone the specified crates from registry or git repository.
    /// Each crate is cloned in a subdirectory named as the crate name.
    /// Returns the cloned crates and the path where they are cloned.
    /// If a crate doesn't exist, is not returned.
    pub async fn clone(&self, crates: &[Crate]) -> CargoResult<Vec<(Package, Utf8PathBuf)>> {
        let _lock = self.acquire_cargo_package_cache_lock()?;
        let src = self.get_source()?;
        let mut destinations = vec![];

        for crate_ in crates {
            let mut dest_path = self.directory.clone();

            dest_path.push(&crate_.name);

            let summary = Self::query_package(crate_, &dest_path, src.as_ref())
                .await
                .with_context(|| {
                    format!("failed to clone package {} in {dest_path}", crate_.name)
                })?;

            if let Some(summary) = summary {
                destinations.push((summary.package_id(), dest_path));
            }
        }

        if destinations.is_empty() {
            return Ok(vec![]);
        }

        let package_ids: Vec<_> = destinations.iter().map(|(id, _)| *id).collect();
        let mut sources = SourceMap::new();
        sources.insert(src);
        let package_set = PackageSet::new(&package_ids, sources, &self.config)?;
        // Cargo downloads this batch concurrently under the package-cache lock. Use the
        // set's unique IDs so repeated requests cannot download the same package twice.
        package_set
            .get_many(package_set.package_ids())
            .context("failed to download packages")?;

        let mut cloned_pkgs = Vec::with_capacity(destinations.len());
        // Downloads finish in any order. Read Cargo's populated cache in request order.
        for (id, dest_path) in destinations {
            let pkg = package_set.get_one(id)?;
            self.clone_package_into(pkg, &dest_path)
                .with_context(|| format!("failed to clone package {} in {dest_path}", id.name()))?;
            cloned_pkgs.push((pkg.clone(), dest_path));
        }

        Ok(cloned_pkgs)
    }

    fn acquire_cargo_package_cache_lock(
        &self,
    ) -> CargoResult<cargo::util::cache_lock::CacheLock<'_>> {
        self.config
            .acquire_package_cache_lock(CacheLockMode::DownloadExclusive)
    }

    fn get_source(&self) -> CargoResult<Box<dyn Source + '_>> {
        let source = if self.srcid.is_path() {
            let path = self.srcid.url().to_file_path().expect("path must be valid");
            Box::new(PathSource::new(&path, self.srcid, &self.config))
        } else {
            let map = SourceConfigMap::new(&self.config)?;
            map.load(self.srcid)?
        };

        source.invalidate_cache();
        Ok(source)
    }

    async fn query_package(
        crate_: &Crate,
        dest_path: &Utf8Path,
        src: &dyn Source,
    ) -> CargoResult<Option<IndexSummary>> {
        prepare_destination(dest_path)?;
        let name = &crate_.name;
        let vers = crate_.version.as_deref();
        let latest = query_latest_package_summary(src, name, vers).await?;

        if latest.is_none() {
            warn!("Package `{}@{}` not found", name, vers.unwrap_or("*.*.*"));
        }
        Ok(latest)
    }
}

fn prepare_destination(dest_path: &Utf8Path) -> CargoResult<()> {
    if !dest_path.exists() {
        fs_err::create_dir_all(dest_path)?;
    }

    // Cloning into an existing directory is only allowed if the directory is empty.
    if dest_path.read_dir()?.next().is_some() {
        bail!("destination path '{dest_path}' already exists and is not an empty directory.");
    }
    Ok(())
}

async fn query_latest_package_summary(
    src: &dyn Source,
    name: &str,
    vers: Option<&str>,
) -> CargoResult<Option<IndexSummary>> {
    let dep = Dependency::parse(name, vers, src.source_id())?;
    let summaries = match src.query_vec(&dep, QueryKind::Exact).await {
        Ok(summaries) => summaries,
        Err(err) => return none_or_query_err(err),
    };
    Ok(summaries
        .into_iter()
        .max_by_key(|summary| summary.package_id().version()))
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
