mod git_tags;
mod registry;

use crate::fs_utils::Utf8TempDir;
use crate::published_packages::git_tags::GitTagsSource;
use crate::published_packages::registry::RegistrySource;
use crate::Project;
use anyhow::Context;
use cargo_metadata::{camino::Utf8Path, semver::Version, Package};
use git_cmd::Repo;
use std::collections::BTreeMap;

/// A collection of [`PublishedPackage`]s.
pub struct PackagesCollection {
    packages: BTreeMap<String, PublishedPackage>,
    /// Packages might be downloaded and stored in a temporary directory.
    /// The directory is stored here so that it is deleted on drop
    _temp_dir: Option<Utf8TempDir>,
}

/// A published [`Package`]'s manifest.
pub struct PublishedPackage {
    pub package: Package,
    /// The SHA1 hash of the commit when the package was published.
    sha1: Option<String>,
}

impl PublishedPackage {
    pub fn published_at_sha1(&self) -> Option<&str> {
        self.sha1.as_deref()
    }
}

impl PackagesCollection {
    pub fn get_package(&self, package_name: &str) -> Option<&Package> {
        self.packages.get(package_name).map(|p| &p.package)
    }

    pub fn get_published_package(&self, package_name: &str) -> Option<&PublishedPackage> {
        self.packages.get(package_name)
    }

    /// Retrieves the latest [`PublishedPackage`]s for each of the given packages.
    ///
    /// If `registry_manifest` is provided, registry packages are read from a manifest in the
    /// local file system. This is useful when the packages are already downloaded.
    /// Otherwise, the packages are downloaded from a cargo registry.
    ///
    /// If `registry` is provided, the packages are downloaded from the specified registry.
    /// Otherwise, the registry specified in each package's manifest is used.
    pub fn fetch_latest<'p>(
        project: &Project,
        repo: &Repo,
        packages: impl Iterator<Item = &'p Package>,
        registry_manifest: Option<&Utf8Path>,
        registry: Option<&str>,
    ) -> anyhow::Result<Self> {
        let temp_dir = Utf8TempDir::new()?;
        let git_tags_source = GitTagsSource::new(project, repo);
        let registry_source = RegistrySource::new(registry_manifest, registry)?;

        let published_packages = packages
            .map(|package| {
                let latest_tag_package = git_tags_source.query_latest(&package.name)?;
                let latest_registry_package = registry_source.query_latest(&package.name)?;

                // TODO: Use registry or tagged version, depending on whether release-plz is set to publish the package to the registry or not
                // TODO: Add `publish` bool to ReleaseMetadata
                let published_package_summary = latest_tag_package
                    .as_ref()
                    .map(|r| r as &dyn Summary)
                    .into_iter()
                    .chain(latest_registry_package.as_ref().map(|r| r as &dyn Summary))
                    .max_by(|a, b| a.version().cmp(b.version()));

                let published_package = published_package_summary
                    .map(|summary| {
                        let dir_name = format!("{}-{}", summary.name(), summary.version());
                        let package_dir = temp_dir.path().join(dir_name);
                        fs_err::create_dir_all(&package_dir)
                            .context("failed to create package dir in temp dir")?;
                        summary.resolve(&package_dir)
                    })
                    .transpose()?;

                Ok(published_package.map(|package| (package.package.name.clone(), package)))
            })
            .filter_map(|x| x.transpose())
            .collect::<anyhow::Result<_>>()?;

        // Restore the repo to its original state
        repo.checkout_head()?;

        Ok(Self {
            packages: published_packages,
            _temp_dir: Some(temp_dir),
        })
    }
}

/// Represents a source of published packages.
trait Source {
    /// Returns a [`Summary`] of the latest published package with the given name.
    fn query_latest<'a>(
        &'a self,
        package_name: &'a str,
    ) -> anyhow::Result<Option<impl Summary + 'a>>;
}

/// A summary of a published package.
///
/// Some properties of the published package can be queried directly through the [`Summary`],
/// but it must be [resolved](Summary::resolve) to a [`PublishedPackage`] for everything else.
trait Summary {
    /// The name of the published package.
    fn name(&self) -> &str;

    /// The version of the published package.
    fn version(&self) -> &Version;

    /// Resolves this [`Summary`] into a [`PublishedPackage`], cloning it into the `temp_dir`
    /// if needed.
    fn resolve(&self, temp_dir: &Utf8Path) -> anyhow::Result<PublishedPackage>;
}
