use super::{PublishedPackage, Source, Summary};
use crate::clone::{Cloner, ClonerSource};
use crate::{cargo_vcs_info, download, next_ver, PackagePath};
use anyhow::Context;
use cargo_metadata::camino::Utf8Path;
use cargo_metadata::semver::Version;
use cargo_metadata::Package;
use git_cmd::git_in_dir;

pub struct RegistrySource<'a> {
    registry_manifest: Option<&'a Utf8Path>,
    cloner: Cloner,
}

impl<'a> RegistrySource<'a> {
    pub fn new(
        registry_manifest: Option<&'a Utf8Path>,
        registry: Option<&str>,
    ) -> anyhow::Result<Self> {
        let source = registry.map_or_else(ClonerSource::crates_io, ClonerSource::registry);

        let cloner = Cloner::builder()
            .with_source(source)
            .build()
            .context("can't build cloner")?;

        Ok(Self {
            registry_manifest,
            cloner,
        })
    }
}

impl Source for RegistrySource<'_> {
    fn query_latest<'a>(
        &'a self,
        package_name: &'a str,
    ) -> anyhow::Result<Option<impl Summary + 'a>> {
        let package_ref = match self.registry_manifest {
            Some(manifest) => next_ver::publishable_packages_from_manifest(manifest)
                .context("failed to load registry manifest")?
                .into_iter()
                .find(|p| p.name == package_name)
                .map(RegistrySummary::FromLocalManifest),
            None => self
                .cloner
                .query_latest_package(package_name)?
                .map(|summary| RegistrySummary::FromRegistry(&self.cloner, summary)),
        };

        Ok(package_ref)
    }
}

enum RegistrySummary<'a> {
    FromLocalManifest(Package),
    FromRegistry(&'a Cloner, cargo::sources::IndexSummary),
}

impl Summary for RegistrySummary<'_> {
    fn name(&self) -> &str {
        match self {
            RegistrySummary::FromLocalManifest(package) => &package.name,
            RegistrySummary::FromRegistry(_, summary) => summary.as_summary().name().as_str(),
        }
    }

    fn version(&self) -> &Version {
        match self {
            RegistrySummary::FromLocalManifest(package) => &package.version,
            RegistrySummary::FromRegistry(_, summary) => summary.as_summary().version(),
        }
    }

    fn resolve(&self, temp_dir: &Utf8Path) -> anyhow::Result<PublishedPackage> {
        match self {
            RegistrySummary::FromLocalManifest(package) => Ok(PublishedPackage {
                package: package.clone(),
                sha1: None,
            }),
            RegistrySummary::FromRegistry(cloner, summary) => {
                cloner.clone_from_summary_into(summary, temp_dir)?;
                let package = download::read_package(temp_dir)?;
                initialize_registry_package(package)
            }
        }
    }
}

fn initialize_registry_package(p: Package) -> anyhow::Result<PublishedPackage> {
    let package_path = p.package_path()?;
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
    Ok(PublishedPackage { package: p, sha1 })
}
