use super::{PublishedPackage, Source, Summary};
use crate::fs_utils::Utf8TempDir;
use crate::Project;
use anyhow::Context;
use cargo::core::{Package, Workspace};
use cargo::GlobalContext;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::semver::Version;
use git_cmd::Repo;
use itertools::Itertools;
use regex::Regex;
use std::path::PathBuf;

/// Utility trait to map nested [`Option`]s and [`Result`]s via [`InnerMap::inner_map`].
trait InnerMap<T> {
    type Output<R>;

    fn inner_map<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R>;
}

impl<T, E> InnerMap<T> for Result<Option<T>, E> {
    type Output<R> = Result<Option<R>, E>;

    fn inner_map<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R> {
        self.map(|inner| inner.map(f))
    }
}

pub struct GitTagsSource<'a> {
    project: &'a Project,
    repo: &'a Repo,
    tags: Vec<String>,
    relative_manifest_dir: &'a Utf8Path,
}

impl<'a> GitTagsSource<'a> {
    pub(crate) fn new(project: &'a Project, repo: &'a Repo) -> Self {
        let relative_manifest_dir = project
            .manifest_dir()
            .strip_prefix(project.root())
            .expect("bug: manifest dir should be a subdirectory of project root");

        Self {
            project,
            repo,
            tags: repo.get_all_tags(),
            relative_manifest_dir,
        }
    }

    /// Checks that the given `tag` is a valid release tag for a package with the given name and
    /// version.
    ///
    /// If `tag` is a valid release tag, returns the path to the workspace manifest at that tag.
    /// Otherwise, returns [`None`].
    fn check_release_tag_validity(
        &self,
        tag: &str,
        package_name: &str,
        version: &Version,
    ) -> anyhow::Result<Option<Utf8PathBuf>> {
        self.repo
            .checkout(tag)
            .with_context(|| format!("failed to checkout release tag `{tag}`"))?;

        // TODO: Workspace manifest may be in a different location in the repository
        // at the release tag than at the current repository HEAD.
        // Maybe do a breadth-first search for the workspace manifest in the repository tree
        let relative_manifest_path = self.relative_manifest_dir.join(cargo_utils::CARGO_TOML);
        let manifest_path = self.repo.directory().join(&relative_manifest_path);

        let metadata = cargo_utils::get_manifest_metadata(&manifest_path).with_context(|| {
            format!(
                "failed to load workspace manifest at path {relative_manifest_path} at tag `{tag}`"
            )
        })?;

        let package_found = cargo_utils::workspace_members(&metadata)
            .with_context(|| format!("failed to get workspace members at tag `{tag}`"))?
            .any(|package| package.name == package_name && package.version == *version);

        if package_found {
            Ok(Some(relative_manifest_path))
        } else {
            tracing::warn!(
                "Tag `{}` looks like a release tag for package `{}` with version `{}`, \
                but the workspace at that tag does not contain a package with that \
                name and version. Treating the tag as not a release tag.",
                tag,
                package_name,
                version
            );
            Ok(None)
        }
    }
}

impl Source for GitTagsSource<'_> {
    fn query_latest<'a>(
        &'a self,
        package_name: &'a str,
    ) -> anyhow::Result<Option<impl Summary + 'a>> {
        // Find the package release tag corresponding to the greatest (i.e. latest) version
        filter_release_tags(
            self.tags.iter().map(AsRef::as_ref),
            package_name,
            self.project,
        )
        .filter_map(|(tag, version)| {
            self.check_release_tag_validity(tag, package_name, &version)
                .inner_map(|relative_manifest_path| (tag, version, relative_manifest_path))
                .transpose()
        })
        .process_results(|tags| {
            tags.max_by(|(_, version1, _), (_, version2, _)| version1.cmp(version2))
        })
        .inner_map(|(tag, version, relative_manifest_path)| ReleaseTag {
            package_name,
            repo: self.repo,
            tag,
            version,
            relative_manifest_path,
        })
    }
}

/// Filters the release tags for the given package from all the `tags` in a repository.
///
/// Each item in the returned iterator is a tuple containing the name of the release tag and the
/// package version it corresponds to.
fn filter_release_tags<'t>(
    tags: impl Iterator<Item = &'t str> + 't,
    package: &'t str,
    project: &'t Project,
) -> impl Iterator<Item = (&'t str, Version)> + 't {
    // TODO: Consider using git tag template in the release-plz config at each tag, rather than
    // using the current template

    let version_var = crate::tera::tera_var(crate::tera::VERSION_VAR);

    // By substituting the version variable expression for the version variable,
    // we only render the package name (if needed) in the template
    let partial_template = project.git_tag(package, &version_var);

    // Escape the partially rendered template so that it can be used as a regex literal
    let fully_escaped_partial_template = regex::escape(&partial_template);

    // Replace the escaped version variable expression substring with the original un-escaped
    // expression string so we can use it as a template
    let escaped_partial_template =
        fully_escaped_partial_template.replace(&regex::escape(&version_var), &version_var);

    // Render template with version = "(.+)" to generate a regex which
    // captures the version string in a group
    let context = crate::tera::tera_context(package, r"(.+)");
    let release_tag_regex =
        crate::tera::render_template(&escaped_partial_template, &context, "release_tag_regex");
    // Add anchors to ensure regex matches whole string
    let release_tag_regex =
        Regex::new(&format!("^{release_tag_regex}$")).expect("invalid rendered version tag regex");

    tags.filter_map(move |tag| {
        // Check if the tag name matches the regex
        let (_, [version_str]) = release_tag_regex.captures(tag)?.extract();
        // Check if the captured version string can be parsed as a package version
        Some((tag, Version::parse(version_str).ok()?))
    })
}

#[derive(Debug)]
struct ReleaseTag<'a> {
    package_name: &'a str,
    repo: &'a Repo,
    tag: &'a str,
    version: Version,
    relative_manifest_path: Utf8PathBuf,
}

const CARGO_TOML_ORIG: &str = "Cargo.toml.orig";
const CARGO_LOCK: &str = "Cargo.lock";

impl Summary for ReleaseTag<'_> {
    fn name(&self) -> &str {
        self.package_name
    }

    fn version(&self) -> &Version {
        &self.version
    }

    #[tracing::instrument]
    fn resolve(&self, temp_dir: &Utf8Path) -> anyhow::Result<PublishedPackage> {
        anyhow::ensure!(
            temp_dir
                .read_dir()
                .context("could not access temp dir")?
                .next()
                .is_none(),
            "temp dir not empty"
        );

        let source_temp_dir = Utf8TempDir::new().context("could not create temp dir")?;
        let source_dir = source_temp_dir.path();

        // Create a new git worktree pointing to the release tag.
        // This lets us easily checkout the repo and the package source at the release tag.
        self.repo.add_worktree(source_dir, self.tag)?;

        let published_package_files = self.copy_package_files(source_dir, temp_dir)?;
        tracing::debug!("published package contents: {:?}", published_package_files);

        // Delete worktree after we are done packaging the crate
        self.repo
            .git(&["worktree", "remove", source_dir.as_str()])
            .context("failed to remove worktree")?;

        let published_package = crate::download::read_package(temp_dir).with_context(|| {
            format!(
                "failed to read package `{}` from extracted .crate",
                self.package_name
            )
        })?;

        Ok(PublishedPackage {
            package: published_package,
            sha1: Some(self.repo.get_tag_commit(self.tag).with_context(|| {
                format!("release tag `{}` does not point to a commit", self.tag)
            })?),
            files: Some(published_package_files),
        })
    }
}

/// A file to be included in the generated package.
struct PackageFile {
    /// The relative path under the package root to put the file.
    relative_path: Utf8PathBuf,
    /// The file contents.
    contents: Contents,
}

/// Represents the contents of a [`PackageFile`].
enum Contents {
    /// A physical file on disk which is copied as-is.
    FromFile(PathBuf),
    /// A manually generated file.
    Generated(GeneratedFile),
}

enum GeneratedFile {
    /// A generated normalized `Cargo.toml`.
    NormalizedManifest,
    /// A generated `Cargo.lock`.
    Lockfile,
}

impl ReleaseTag<'_> {
    /// Copies all the package files from the workspace in `source_dir` to `target_dir`.
    /// Returns a list of relative paths (relative to `target_dir`) of all package files that
    /// were copied or generated.
    ///
    /// This function aims to replicate most of what `cargo package` does in terms of collecting
    /// and generating files to include in a package. Because `cargo package` removes any path
    /// components from dependencies before generating the package, it cannot be used in workspaces
    /// that contain unpublished packages that can *only* be included via a path dependency.
    fn copy_package_files(
        &self,
        source_dir: &Utf8Path,
        target_dir: &Utf8Path,
    ) -> anyhow::Result<Vec<Utf8PathBuf>> {
        let gctx = crate::cargo::new_global_context_in(Some(source_dir.to_path_buf()))
            .context("failed to create Cargo config")?;

        let manifest_path = source_dir.join(&self.relative_manifest_path);
        let workspace = Workspace::new(manifest_path.as_std_path(), &gctx)
            .context("failed to load workspace manifest")?;

        let package = workspace
            .members()
            .find(|package| package.name() == self.package_name)
            .with_context(|| {
                format!(
                    "failed to find package `{}` in workspace at release tag `{}`",
                    self.package_name, self.tag
                )
            })?;

        let published_package_files = get_package_files(&gctx, package)
            .with_context(|| format!("release tag `{}`", self.tag))?;

        for package_file in &published_package_files {
            let relative_path = &package_file.relative_path;
            let dest_path = target_dir.as_std_path().join(relative_path);

            match &package_file.contents {
                Contents::FromFile(package_file) => {
                    tracing::trace!(
                        "copying package file {} to {}",
                        package_file.display(),
                        dest_path.display()
                    );

                    anyhow::ensure!(
                        package_file.is_absolute(),
                        "bug: package file source path must be absolute"
                    );

                    fs_err::create_dir_all(
                        dest_path.parent().context("bug: dest_path has no parent")?,
                    )?;
                    fs_err::copy(package_file, &dest_path)?;
                }
                Contents::Generated(generated) => {
                    let contents = match generated {
                        GeneratedFile::NormalizedManifest => package
                            .manifest()
                            .to_normalized_contents()
                            .context("failed to normalize Cargo.toml")?,
                        GeneratedFile::Lockfile => {
                            generate_lockfile_for_package(&workspace, package)?
                        }
                    };

                    fs_err::write(&dest_path, &contents)?;
                    tracing::trace!("wrote {relative_path} contents:\n{}", contents);
                }
            }
        }

        Ok(published_package_files
            .into_iter()
            .map(|package_file| package_file.relative_path)
            .collect())
    }
}

/// Returns all the [`PackageFile`]s, both physical and generated, in the given package.
fn get_package_files(gctx: &GlobalContext, package: &Package) -> anyhow::Result<Vec<PackageFile>> {
    let package_root = package.root();

    // Get all the files in the package root directory that we should include in the package.
    // This is pretty much what is included in `cargo package --list`, except for
    // some autogenerated files like Cargo.toml.orig and .cargo_vcs_info.json.
    // `cargo package` also calls this method.
    let package_files = cargo::sources::path::list_files(package, gctx).with_context(|| {
        format!(
            "failed to get package files for package `{}`",
            package.name()
        )
    })?;

    tracing::debug!("package files: {:?}", package_files);

    let mut published_package_files = vec![];
    let mut cargo_toml_path = None;

    for package_file in package_files {
        let Ok(relative_path) = package_file.strip_prefix(package_root) else {
            tracing::warn!(
                "Package file `{}` is not contained in package root directory `{}`. \
                    This is likely a bug.",
                package_file.display(),
                package_root.display()
            );
            continue;
        };

        let Some(relative_path) = relative_path.to_str().map(Utf8PathBuf::from) else {
            tracing::warn!(
                "skipping non-UTF-8 package file path `{}`",
                relative_path.display()
            );
            continue;
        };

        match relative_path.as_str() {
            CARGO_TOML_ORIG => {
                tracing::warn!(
                    "Package source includes reserved file name `{}`. Skipping this entry.",
                    relative_path,
                );
            }
            CARGO_LOCK => {
                tracing::debug!("skipping {CARGO_LOCK} from repo in favor of the one we generate");
            }
            cargo_utils::CARGO_TOML => cargo_toml_path = Some(package_file),
            _ => {
                published_package_files.push(PackageFile {
                    relative_path,
                    contents: Contents::FromFile(package_file),
                });
            }
        }
    }

    let Some(cargo_toml_path) = cargo_toml_path else {
        anyhow::bail!(
            "no `Cargo.toml` file found for package `{}`",
            package.name(),
        );
    };

    // We compare the local package's Cargo.toml with the original Cargo.toml
    // (stored in Cargo.toml.orig) of the published package.
    // Copy the Cargo.toml to Cargo.toml.orig and normalize the Cargo.toml so it can be read
    // in isolation outside the workspace.
    // The normalized manifest may still contain path dependencies that cannot be resolved
    // outside the workspace, but this should not matter since we generate the Cargo.lock
    // inside the workspace here (if needed), and only need the manifest to be readable.
    tracing::trace!("Copying Cargo.toml to Cargo.toml.orig");

    published_package_files.push(PackageFile {
        relative_path: CARGO_TOML_ORIG.into(),
        contents: Contents::FromFile(cargo_toml_path),
    });

    published_package_files.push(PackageFile {
        relative_path: cargo_utils::CARGO_TOML.into(),
        contents: Contents::Generated(GeneratedFile::NormalizedManifest),
    });

    // cargo v0.85.0 onwards always generates the lock file, so we do too
    published_package_files.push(PackageFile {
        relative_path: CARGO_LOCK.into(),
        contents: Contents::Generated(GeneratedFile::Lockfile),
    });

    Ok(published_package_files)
}

/// Generate the lock file to be included in the package and returns its contents.
///
/// See [`cargo::ops::cargo_package::build_lock`].
fn generate_lockfile_for_package(
    workspace: &Workspace,
    package: &Package,
) -> anyhow::Result<String> {
    // Use resolve information already contained in the workspace lock file.
    let orig_resolve = cargo::ops::load_pkg_lockfile(workspace)?;

    // Create an ephemeral workspace containing the single package we are packaging
    // This prevents packages in the workspace lock file that not required by this package
    // from being included in the generated lock file.
    let resolve_ws = Workspace::ephemeral(package.clone(), workspace.gctx(), None, true)?;
    let mut package_registry = resolve_ws.package_registry()?;

    let new_resolve = cargo::ops::resolve_with_previous(
        &mut package_registry,
        &resolve_ws,
        &cargo::core::resolver::CliFeatures::new_all(true),
        cargo::core::resolver::features::HasDevUnits::Yes,
        orig_resolve.as_ref(),
        None,
        &[],
        true,
    )?;

    cargo::ops::resolve_to_string(&resolve_ws, &new_resolve)
}
