// Copied from [cargo-clone](https://github.com/JanLikar/cargo-clone/blob/89ba4da215663ffb3b8c93a674f3002937eafec4/cargo-clone-core/src/lib.rs)
//! Fetch the source code of a Rust crate from a registry.

use cargo::util::cache_lock::CacheLockMode;
use cargo_metadata::camino::Utf8Path;
use cargo_metadata::camino::Utf8PathBuf;
use futures::StreamExt as _;
use futures::stream::FuturesOrdered;
use tracing::warn;

use anyhow::{Context, bail};

use cargo::sources::source::{QueryKind, Source, SourceMap};
use cargo::sources::{IndexSummary, SourceConfigMap};
use cargo::util::{CargoResult, GlobalContext};
use cargo::workspace::{Dependency, Package, PackageId, PackageSet, SourceId};

use walkdir::WalkDir;

use crate::cargo::new_cargo_config;
use crate::fs_utils::strip_prefix;
use crate::fs_utils::to_utf8_path;

/// Clones a crate.
pub struct Cloner {
    /// Cargo configuration.
    config: GlobalContext,
    /// Directory where the crates will be cloned.
    /// Each crate is cloned into a subdirectory of this directory.
    directory: Utf8PathBuf,
    /// Where the crates will be cloned from.
    srcid: SourceId,
}

impl Cloner {
    /// Creates a [`Cloner`] that clones crates from `registry`, or from crates.io if `None`.
    /// `cargo_cwd` is the Cargo working directory: use it to point to the right `.cargo/config.toml`.
    pub fn new(
        directory: impl Into<Utf8PathBuf>,
        registry: Option<&str>,
        cargo_cwd: Option<Utf8PathBuf>,
    ) -> CargoResult<Self> {
        let config = new_cargo_config(cargo_cwd).context("Unable to get cargo config.")?;
        let srcid = match registry {
            Some(registry) => SourceId::alt_registry(&config, registry),
            None => SourceId::crates_io(&config),
        }
        .context("can't determine the source id")?;
        Ok(Self {
            config,
            directory: directory.into(),
            srcid,
        })
    }

    /// Clone the specified crates from the registry.
    /// Each crate is cloned in a subdirectory named as the crate name.
    /// Returns the paths where the crates are cloned.
    /// If a crate doesn't exist, its path is not returned.
    pub async fn clone(&self, names: &[String]) -> CargoResult<Vec<Utf8PathBuf>> {
        let _lock = self.acquire_cargo_package_cache_lock()?;
        let src = self.get_source()?;

        let destinations = self.query_destinations(src.as_ref(), names).await?;
        if destinations.is_empty() {
            return Ok(vec![]);
        }

        let package_ids: Vec<_> = destinations.iter().map(|(id, _)| *id).collect();
        let mut sources = SourceMap::new();
        sources.insert(src);
        let package_set = PackageSet::new(&package_ids, sources, &self.config)?;
        // Use the set's deduplicated IDs: Cargo panics if a batch downloads the same ID twice.
        package_set.get_many(package_set.package_ids())?;

        // `get_many` returns packages in completion order.
        // Look each one up by ID to keep request order.
        destinations
            .into_iter()
            .map(|(id, dest_path)| {
                let pkg = package_set.get_one(id)?;
                clone_package_into(pkg, &dest_path).with_context(|| {
                    format!("failed to clone package {} in {dest_path}", id.name())
                })?;
                Ok(dest_path)
            })
            .collect()
    }

    /// Query the latest version of each crate and check the directory it will be cloned into.
    /// Crates that don't exist are skipped.
    async fn query_destinations(
        &self,
        src: &dyn Source,
        names: &[String],
    ) -> CargoResult<Vec<(PackageId, Utf8PathBuf)>> {
        // `get_source` invalidated the index cache, so sparse registries revalidate every
        // index file with the server. Cargo runs these requests in parallel, so poll the
        // queries together instead of paying one round trip per crate.
        // Stop at the first error in request order: Cargo doesn't keep a failed sparse
        // `config.json`, so without a copy on disk every pending query would fetch it
        // again, one after another.
        let mut summaries: FuturesOrdered<_> = names
            .iter()
            .map(async |name| (name, query_latest_package_summary(src, name).await))
            .collect();

        let mut destinations = vec![];
        while let Some((name, summary)) = summaries.next().await {
            match summary.with_context(|| format!("failed to query package {name}"))? {
                Some(summary) => {
                    let dest_path = self.directory.join(name);
                    check_destination(&dest_path).with_context(|| {
                        format!("failed to clone package {name} in {dest_path}")
                    })?;
                    destinations.push((summary.package_id(), dest_path));
                }
                None => warn!("Package `{name}` not found"),
            }
        }
        Ok(destinations)
    }

    fn acquire_cargo_package_cache_lock(
        &self,
    ) -> CargoResult<cargo::util::cache_lock::CacheLock<'_>> {
        self.config
            .acquire_package_cache_lock(CacheLockMode::DownloadExclusive)
    }

    fn get_source(&self) -> CargoResult<Box<dyn Source + '_>> {
        let map = SourceConfigMap::new(&self.config)?;
        let source = map.load(self.srcid)?;
        source.invalidate_cache();
        Ok(source)
    }
}

fn clone_package_into(pkg: &Package, dest_path: &Utf8Path) -> CargoResult<()> {
    // Duplicate requests share a destination that an earlier copy already filled.
    check_destination(dest_path)?;
    fs_err::create_dir_all(dest_path)?;
    clone_directory(to_utf8_path(pkg.root())?, dest_path).context("failed to clone directory")
}

fn check_destination(dest_path: &Utf8Path) -> CargoResult<()> {
    // Cloning into an existing directory is only allowed if the directory is empty.
    if dest_path.exists() && dest_path.read_dir()?.next().is_some() {
        bail!("destination path '{dest_path}' already exists and is not an empty directory.");
    }
    Ok(())
}

async fn query_latest_package_summary(
    src: &dyn Source,
    name: &str,
) -> CargoResult<Option<IndexSummary>> {
    let dep = Dependency::parse(name, None, src.source_id())?;
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

        // .cargo-ok is not wanted in this context
        if entry.file_name() == ".cargo-ok" {
            continue;
        }

        if !file_type.is_dir() {
            fs_err::copy(entry.path(), &dest_path)?;
        } else if dest_path != to {
            fs_err::create_dir(&dest_path)?;
        }
    }

    Ok(())
}
