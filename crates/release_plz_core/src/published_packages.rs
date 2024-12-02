use crate::{cargo_vcs_info, download, next_ver, PackagePath, Project};
use anyhow::Context;
use cargo_metadata::{camino::Utf8Path, Package};
use git_cmd::{git_in_dir, Repo};
use itertools::Itertools;
use regex::Regex;
use std::collections::BTreeMap;
use tempfile::{tempdir, TempDir};

/// A collection of [`PublishedPackage`]s.
pub struct PackagesCollection {
    packages: BTreeMap<String, PublishedPackage>,
    /// Packages might be downloaded and stored in a temporary directory.
    /// The directory is stored here so that it is deleted on drop
    temp_dir: Option<TempDir>,
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

    fn new() -> Self {
        Self {
            packages: BTreeMap::default(),
            temp_dir: None,
        }
    }

    fn temp_dir(&mut self) -> anyhow::Result<&TempDir> {
        if self.temp_dir.is_none() {
            self.temp_dir = Some(tempdir().context("failed to get a temporary directory")?);
        }

        Ok(self.temp_dir.as_ref().unwrap())
    }

    fn push(&mut self, package: PublishedPackage) {
        self.packages.insert(package.package.name.clone(), package);
    }

    fn extend(&mut self, packages: impl IntoIterator<Item = PublishedPackage>) {
        self.packages
            .extend(packages.into_iter().map(|p| (p.package.name.clone(), p)));
    }

    /// Retrieve the latest version of the packages from a registry.
    ///
    /// - If `registry_manifest` is provided, the packages are read from the local file system.
    ///   This is useful when the packages are already downloaded.
    /// - Otherwise, the packages are downloaded from the cargo registry.
    ///
    /// - If `registry` is provided, the packages are downloaded from the specified registry.
    /// - Otherwise, the packages are downloaded from crates.io.
    fn get_registry_packages<'p>(
        &mut self,
        registry_manifest: Option<&Utf8Path>,
        local_packages: impl IntoIterator<Item = &'p Package>,
        registry: Option<&str>,
    ) -> anyhow::Result<()> {
        match registry_manifest {
            Some(manifest) => self.extend(
                next_ver::publishable_packages_from_manifest(manifest)?
                    .into_iter()
                    .map(|p| PublishedPackage {
                        package: p,
                        sha1: None,
                    }),
            ),
            None => {
                let temp_dir = self.temp_dir()?;
                let directory = temp_dir.as_ref().to_str().context("invalid tempdir path")?;

                // Find the registry from where to download each package.
                let packages_grouped_by_registry = local_packages.into_iter().chunk_by(|p| {
                    // If registry is not provided, fallback to the Cargo.toml `publish` field.
                    registry.or_else(|| {
                        p.publish
                         .as_ref()
                            // Use the first registry in the `publish` field.
                         .and_then(|p| p.first())
                         .map(|x| x.as_str())
                    })
                });
                let mut registry_packages: Vec<Package> = vec![];
                for (registry, packages) in &packages_grouped_by_registry {
                    let packages_names: Vec<&str> = packages.map(|p| p.name.as_str()).collect();
                    let mut downloader =
                        download::PackageDownloader::new(packages_names, directory);
                    if let Some(registry) = registry {
                        downloader = downloader.with_registry(registry.to_string());
                    }
                    registry_packages.extend(
                        downloader
                            .download()
                            .context("failed to download packages")?,
                    );
                }

                // After downloading the package, we initialize a git repo in the package.
                // This is because if cargo doesn't find a git repo in the package, it doesn't
                // show hidden files in `cargo package --list` output.
                let registry_packages = initialize_registry_package(registry_packages)
                    .context("failed to initialize repository package")?;
                self.extend(registry_packages);
            }
        }
        Ok(())
    }

    /// Retrieves the latest [`PublishedPackage`]s for the given `packages`
    /// from git tags in the `repo`.
    ///
    /// The returned [`Iterator`] upon consumption may leave the `repo` in a different state
    /// than when it was called; it is the caller's responsibility to restore it to the
    /// original state if needed (for example, with [`Repo::checkout_head`]).
    fn get_latest_tagged_packages<'p>(
        &mut self,
        project: &Project,
        repo: &Repo,
        packages: impl Iterator<Item = &'p Package> + 'p,
    ) -> anyhow::Result<()> {
        let tags = repo.get_tags_version_sorted(true);

        for package in packages {
            // Latest release tag is the first one we find in the descending list of tags
            let Some(release_tag) =
                filter_release_tags(tags.iter().map(AsRef::as_ref), &package.name, project).next()
            else {
                continue;
            };

            let temp_dir = self.temp_dir()?;

            let package_store_dir = temp_dir.path().join(&package.name);

            // "Download" each package into the temp dir.
            // We do this by simply creating a new worktree pointing to the release tag.
            // We could also do this in other ways:
            // 1. Find relative path to package manifest and checkout package contents
            //    (see git read-tree and checkout-index) into temp dir
            // 2. Use `cargo package` to create tarball, and extract it
            // But the simplest is to use a worktree

            repo.add_worktree(&package_store_dir, release_tag)?;

            let metadata =
                cargo_utils::get_manifest_metadata(&repo.directory().join(cargo_utils::CARGO_TOML))
                    .with_context(|| {
                        format!("failed to get root manifest metadata at tag '{release_tag}'")
                    })?;

            let published_package = metadata
                .packages
                .into_iter()
                .find(|p| p.name == package.name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "could not retrieve package '{}' in manifest at tag '{}'",
                        package.name,
                        release_tag
                    )
                })?;

            self.push(PublishedPackage {
                package: published_package,
                sha1: Some(repo.current_commit_hash()?),
            });
        }

        Ok(())
    }
}

/// Retrieves the latest [`PublishedPackage`]s for each of the given packages. The
/// `registry_packages` are looked up in the registry while the `git_only_packages` are
/// looked up via git tags.
pub fn get_latest_packages<'p>(
    project: &'p Project,
    repo: &'p Repo,
    registry_packages: impl IntoIterator<Item = &'p Package, IntoIter: 'p>,
    git_only_packages: impl IntoIterator<Item = &'p Package, IntoIter: 'p>,
    registry_manifest: Option<&Utf8Path>,
    registry: Option<&str>,
) -> anyhow::Result<PackagesCollection> {
    let mut collection = PackagesCollection::new();

    collection.get_registry_packages(registry_manifest, registry_packages, registry)?;

    collection.get_latest_tagged_packages(project, repo, git_only_packages.into_iter())?;

    // Restore the repo to its original state
    repo.checkout_head()?;

    Ok(collection)
}

fn initialize_registry_package(packages: Vec<Package>) -> anyhow::Result<Vec<PublishedPackage>> {
    let mut registry_packages = vec![];
    for p in packages {
        let package_path = p.package_path().unwrap();
        let cargo_vcs_info_path = package_path.join(".cargo_vcs_info.json");
        // cargo_vcs_info is only present if `cargo publish` wasn't used with
        // the `--allow-dirty` flag inside a git repo.
        let sha1 = if cargo_vcs_info_path.exists() {
            let sha1 = cargo_vcs_info::read_sha1_from_cargo_vcs_info(&cargo_vcs_info_path);
            // Remove the file, otherwise `cargo publish --list` fails
            fs_err::remove_file(cargo_vcs_info_path)?;
            sha1
        } else {
            None
        };
        let git_repo = package_path.join(".git");
        let commit_init = || git_in_dir(package_path, &["commit", "-m", "init"]);
        if !git_repo.exists() {
            git_in_dir(package_path, &["init"])?;
            git_in_dir(package_path, &["add", "."])?;
            if let Err(e) = commit_init() {
                if e.to_string().trim().starts_with("Author identity unknown") {
                    // we can use any email and name here, as this repository is only used
                    // to compare packages
                    git_in_dir(package_path, &["config", "user.email", "test@registry"])?;
                    git_in_dir(package_path, &["config", "user.name", "test"])?;
                    commit_init()?;
                }
            }
        }
        registry_packages.push(PublishedPackage { package: p, sha1 });
    }
    Ok(registry_packages)
}

/// Filters the release tags for the given package from all the `tags` in a repository.
fn filter_release_tags<'t>(
    tags: impl Iterator<Item = &'t str> + 't,
    package: &'t str,
    project: &'t Project,
) -> impl Iterator<Item = &'t str> + 't {
    lazy_static::lazy_static! {
        static ref SEMVER_RE: Regex = Regex::new(r"((?-u:\d)+\.(?-u:\d)+\.(?-u:\d)+)").unwrap();
    }

    // TODO: Consider using git tag template in the release-plz config at each tag, rather than
    // using the current template

    tags
        // Find tags that contain a semver version string
        .filter_map(|tag| Some((tag, SEMVER_RE.find(tag)?.as_str())))
        // Render the git tag template for the package with the matched version string
        // and check if the tag matches
        .filter_map(|(tag, version)| (tag == project.git_tag(package, version)).then_some(tag))
}
