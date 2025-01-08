use super::{PublishedPackage, Source, Summary};
use crate::fs_utils::Utf8TempDir;
use crate::Project;
use anyhow::Context;
use cargo::core::Workspace;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::semver::Version;
use git_cmd::Repo;
use regex::Regex;

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
}

impl Source for GitTagsSource<'_> {
    fn query_latest<'a>(
        &'a self,
        package_name: &'a str,
    ) -> anyhow::Result<Option<impl Summary + 'a>> {
        // Find the package release tag corresponding to the greatest (i.e. latest) version
        let release_tag = filter_release_tags(
            self.tags.iter().map(AsRef::as_ref),
            package_name,
            self.project,
        )
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(tag, version)| ReleaseTag {
            package_name,
            repo: self.repo,
            tag,
            version,
            relative_manifest_dir: self.relative_manifest_dir,
        });

        Ok(release_tag)
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
    relative_manifest_dir: &'a Utf8Path,
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
                "failed to read package '{}' from extracted .crate",
                self.package_name
            )
        })?;

        Ok(PublishedPackage {
            package: published_package,
            sha1: Some(self.repo.get_tag_commit(self.tag).with_context(|| {
                format!("release tag '{}' does not point to a commit", self.tag)
            })?),
            files: Some(published_package_files),
        })
    }
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

        // TODO: Workspace manifest may be in a different location in the repository
        // at the release tag than at the current repository HEAD.
        // Maybe do a breadth-first search for the workspace manifest in the repository tree?
        let mut manifest_path = source_dir.join(self.relative_manifest_dir);
        manifest_path.push(cargo_utils::CARGO_TOML);

        let workspace = Workspace::new(manifest_path.as_std_path(), &gctx)
            .context("failed to load workspace manifest")?;

        let package = workspace
            .members()
            .find(|package| package.name() == self.package_name)
            .with_context(|| {
                format!(
                    "failed to find package '{}' in workspace at release tag '{}'",
                    self.package_name, self.tag
                )
            })?;
        let package_root = package.root();

        // Get all the files that we should include in the package.
        // This is pretty much what is included in `cargo package --list`, except for
        // some autogenerated files like Cargo.toml.orig and .cargo_vcs_info.json.
        // `cargo package` also calls this method.
        let package_files =
            cargo::sources::path::list_files(package, &gctx).with_context(|| {
                format!(
                    "failed to get package files for package '{}' at release tag '{}'",
                    self.package_name, self.tag
                )
            })?;

        tracing::debug!("package files: {:?}", package_files);

        let mut published_package_files = vec![];

        // Copy all package source files to destination dir
        for package_file in package_files {
            let Ok(relative_path) = package_file.strip_prefix(package_root) else {
                tracing::warn!(
                    "Package file '{}' is not contained in package root directory '{}'. \
                    This is likely a bug.",
                    package_file.display(),
                    package_root.display()
                );
                continue;
            };

            let relative_path_os_str = relative_path.as_os_str();

            if relative_path_os_str == CARGO_TOML_ORIG {
                tracing::warn!(
                    "Package source includes reserved file name '{}'. Skipping this entry.",
                    relative_path.display(),
                );
                continue;
            } else if relative_path_os_str == CARGO_LOCK {
                tracing::debug!("skipping {CARGO_LOCK} from repo in favor of the one we generate");
                continue;
            }

            let dest_path = target_dir.as_std_path().join(relative_path);

            tracing::trace!(
                "copying package file {} to {}",
                package_file.display(),
                dest_path.display()
            );

            fs_err::create_dir_all(dest_path.parent().context("bug: dest_path has no parent")?)?;
            fs_err::copy(&package_file, &dest_path)?;

            published_package_files.push(
                relative_path
                    .to_path_buf()
                    .try_into()
                    .context("package contains non-UTF-8 path")?,
            );

            // We compare the local package's Cargo.toml with the original Cargo.toml
            // (stored in Cargo.toml.orig) of the published package.
            // Copy the Cargo.toml to Cargo.toml.orig
            if relative_path_os_str == cargo_utils::CARGO_TOML {
                tracing::trace!("Copying Cargo.toml to Cargo.toml.orig");

                fs_err::copy(&package_file, target_dir.join(CARGO_TOML_ORIG))?;
                published_package_files.push(CARGO_TOML_ORIG.into());
            }
        }

        if package.include_lockfile() {
            // Generate the lock file to be included in the package
            // See cargo::ops::cargo_package::build_lock

            // Use resolve information already contained in the workspace lock file.
            let orig_resolve = cargo::ops::load_pkg_lockfile(&workspace)?;

            // Create an ephemeral workspace containing the single package we are packaging
            // This prevents packages in the workspace lock file that not required by this package
            // from being included in the generated lock file.
            let resolve_ws = Workspace::ephemeral(package.clone(), &gctx, None, true)?;
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

            let lockfile_contents = cargo::ops::resolve_to_string(&resolve_ws, &new_resolve)?;
            let cargo_lock_file_name = Utf8Path::new(CARGO_LOCK).to_path_buf();
            fs_err::write(target_dir.join(&cargo_lock_file_name), &lockfile_contents)?;

            published_package_files.push(cargo_lock_file_name);
            tracing::trace!("wrote {CARGO_LOCK} contents:\n{}", lockfile_contents);
        }

        Ok(published_package_files)
    }
}
