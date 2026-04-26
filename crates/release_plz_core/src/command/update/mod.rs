mod changelog_update;
mod package_dependencies;
mod packages_update;
mod update_config;
pub mod update_request;
pub mod updater;

use crate::{PackagePath, tmp_repo::TempRepo};
use crate::{fs_utils, root_repo_path_from_manifest_dir};
use anyhow::Context;
use cargo_metadata::camino::Utf8Path;
use cargo_metadata::{Package, semver::Version};
use cargo_utils::LocalManifest;
use cargo_utils::{CARGO_TOML, upgrade_requirement};
use git_cmd::Repo;
use serde::{Deserialize, Serialize};
use std::iter;
use tracing::{info, warn};
use update_request::UpdateRequest;

use tracing::{debug, instrument};

pub use packages_update::*;
pub use update_config::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct ReleaseInfo {
    /// Package name
    package: String,
    pub title: Option<String>,
    pub changelog: Option<String>,
    previous_version: String,
    next_version: String,
    /// Summary of breaking changes of the release
    breaking_changes: Option<String>,
    semver_check: String,
}

/// Update a local Rust project.
#[instrument(skip_all)]
pub async fn update(input: &UpdateRequest) -> anyhow::Result<(PackagesUpdate, TempRepo)> {
    let (packages_to_update, repository) = crate::next_versions(input)
        .await
        .context("failed to determine next versions")?;
    let local_manifest_path = input.local_manifest();
    let local_metadata = cargo_utils::get_manifest_metadata(local_manifest_path)?;
    // Read packages from `local_metadata` to update the manifest of local
    // workspace dependencies.
    let all_packages: Vec<Package> = cargo_utils::workspace_members(&local_metadata)?.collect();
    let all_packages_ref: Vec<&Package> = all_packages.iter().collect();
    update_manifests(&packages_to_update, local_manifest_path, &all_packages_ref)?;
    update_changelogs(input, &packages_to_update)?;
    if !packages_to_update.updates().is_empty() {
        let local_manifest_dir = input.local_manifest_dir()?;
        update_cargo_lock(local_manifest_dir, input.should_update_dependencies())?;

        let local_repo_root = root_repo_path_from_manifest_dir(local_manifest_dir)?;
        let there_are_commits_to_push = Repo::new(local_repo_root)?.is_clean().is_err();
        if !there_are_commits_to_push {
            info!("the repository is already up-to-date");
        }
    }

    Ok((packages_to_update, repository))
}

fn update_manifests(
    packages_to_update: &PackagesUpdate,
    local_manifest_path: &Utf8Path,
    all_packages: &[&Package],
) -> anyhow::Result<()> {
    // Distinguish packages type to avoid updating the version of packages that inherit the workspace version
    let (workspace_pkgs, independent_pkgs): (PackagesToUpdate, PackagesToUpdate) =
        packages_to_update
            .updates_clone()
            .into_iter()
            .partition(|(p, _)| {
                let local_manifest_path = p.package_path().unwrap().join(CARGO_TOML);
                let local_manifest = LocalManifest::try_new(&local_manifest_path).unwrap();
                local_manifest.version_is_inherited()
            });

    if let Some(new_workspace_version) = packages_to_update.workspace_version() {
        let mut local_manifest = LocalManifest::try_new(local_manifest_path)?;
        local_manifest.set_workspace_version(new_workspace_version);
        local_manifest
            .write()
            .context("can't update workspace version")?;

        for (pkg, _) in workspace_pkgs {
            let package_path = pkg.package_path()?;
            update_dependencies(
                all_packages,
                new_workspace_version,
                package_path,
                local_manifest_path,
            )?;
        }
    }

    update_versions(
        all_packages,
        &PackagesUpdate::new(independent_pkgs),
        local_manifest_path,
    )?;
    Ok(())
}

#[instrument(skip_all)]
fn update_versions(
    all_packages: &[&Package],
    packages_to_update: &PackagesUpdate,
    workspace_manifest: &Utf8Path,
) -> anyhow::Result<()> {
    for (package, update) in packages_to_update.updates() {
        let package_path = package.package_path()?;
        set_version(
            all_packages,
            package_path,
            &update.version,
            workspace_manifest,
        )?;
    }
    Ok(())
}

#[instrument(skip_all)]
fn update_changelogs(
    update_request: &UpdateRequest,
    local_packages: &PackagesUpdate,
) -> anyhow::Result<()> {
    // When workspace_changelog_path is set, aggregate all packages into one file.
    if let Some(ws_path) = update_request.workspace_changelog_path() {
        return update_workspace_changelog(update_request, local_packages, ws_path);
    }
    // Default: write per-crate CHANGELOG.md.
    for (package, update) in local_packages.updates() {
        if let Some(changelog) = update.changelog.as_ref() {
            let changelog_path = update_request.changelog_path(package);
            fs_err::write(&changelog_path, changelog).context("cannot write changelog")?;
        }
    }
    Ok(())
}

fn update_workspace_changelog(
    update_request: &UpdateRequest,
    local_packages: &PackagesUpdate,
    ws_changelog_path: &Utf8Path,
) -> anyhow::Result<()> {
    use chrono::Utc;

    // Collect non-empty new changelog entries sorted by package name.
    let mut pkg_entries: Vec<(String, String, cargo_metadata::semver::Version)> = local_packages
        .updates()
        .iter()
        .filter_map(|(pkg, update)| {
            update.new_changelog_entry.as_ref().and_then(|e| {
                let trimmed = e.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some((pkg.name.to_string(), trimmed, update.version.clone()))
                }
            })
        })
        .collect();

    if pkg_entries.is_empty() {
        return Ok(());
    }

    pkg_entries.sort_by(|a, b| a.0.cmp(&b.0));

    // Determine the release version: use max version across all updated packages.
    let version = pkg_entries
        .iter()
        .map(|(_, _, v)| v)
        .max()
        .cloned()
        .unwrap_or_else(|| cargo_metadata::semver::Version::new(0, 0, 0));

    let date = Utc::now().format("%Y-%m-%d");
    let mut new_entry = format!("## v{version} — {date}\n\n");

    for (pkg_name, entry, _) in &pkg_entries {
        new_entry.push_str(&format!("### {pkg_name}\n"));
        // Demote package-level heading sections: ### Foo → #### Foo
        for line in entry.lines() {
            if let Some(rest) = line.strip_prefix("### ") {
                new_entry.push_str(&format!("#### {rest}\n"));
            } else {
                new_entry.push_str(line);
                new_entry.push('\n');
            }
        }
        new_entry.push('\n');
    }

    // Resolve path relative to workspace root (parent of local_manifest).
    let abs_path = update_request
        .local_manifest()
        .parent()
        .context("cannot determine workspace root from local manifest path")?
        .join(ws_changelog_path);

    let existing = if abs_path.exists() {
        fs_err::read_to_string(&abs_path)?
    } else {
        String::new()
    };

    const HEADER: &str = "# Changelog\n\n";
    let body = existing.strip_prefix(HEADER).unwrap_or(existing.as_str());

    // Separate from previous entry with a horizontal rule.
    let separator = if body.trim().is_empty() {
        String::new()
    } else {
        "---\n\n".to_string()
    };

    let new_content = format!("{HEADER}{new_entry}{separator}{body}");
    fs_err::write(&abs_path, new_content).context("cannot write workspace changelog")?;
    Ok(())
}

#[instrument(skip_all)]
fn update_cargo_lock(root: &Utf8Path, update_all_dependencies: bool) -> anyhow::Result<()> {
    let mut args = vec!["update"];
    if !update_all_dependencies {
        args.push("--workspace");
    }
    let output = crate::cargo::run_cargo(root, &args)
        .context("error while running cargo to update the Cargo.lock file")?;

    anyhow::ensure!(
        output.status.success(),
        "cargo update failed. stdout: {}; stderr: {}",
        output.stdout,
        output.stderr
    );

    Ok(())
}

#[instrument(skip(all_packages))]
pub fn set_version(
    all_packages: &[&Package],
    package_path: &Utf8Path,
    version: &Version,
    workspace_manifest: &Utf8Path,
) -> anyhow::Result<()> {
    debug!("updating version");
    let mut local_manifest =
        LocalManifest::try_new(&package_path.join("Cargo.toml")).context("cannot read manifest")?;
    local_manifest.set_package_version(version);
    local_manifest
        .write()
        .with_context(|| format!("cannot update manifest {:?}", &local_manifest.path))?;

    let package_path = fs_utils::canonicalize_utf8(crate::manifest_dir(&local_manifest.path)?)?;
    update_dependencies(all_packages, version, &package_path, workspace_manifest)?;
    Ok(())
}

/// Update the package version in the dependencies of the other packages.
/// E.g. from:
///
/// ```toml
/// [dependencies]
/// pkg1 = { path = "../pkg1", version = "1.2.3" }
/// ```
///
/// to:
///
/// ```toml
/// [dependencies]
/// pkg1 = { path = "../pkg1", version = "1.2.4" }
/// ```
///
/// Works also for the dependencies in a workspace:
///
/// ```toml
/// [workspace.dependencies]
/// pkg1 = { path = "../pkg1", version = "1.2.4" }
/// ```
///
fn update_dependencies(
    all_packages: &[&Package],
    version: &Version,
    package_path: &Utf8Path,
    workspace_manifest: &Utf8Path,
) -> anyhow::Result<()> {
    let all_manifests = iter::once(workspace_manifest)
        .chain(all_packages.iter().map(|pkg| pkg.manifest_path.as_path()));
    for manifest in all_manifests {
        let mut local_manifest = LocalManifest::try_new(manifest)?;
        let manifest_dir = crate::manifest_dir(&local_manifest.path)?.to_owned();
        let deps_to_update = local_manifest
            .get_dependency_tables_mut()
            .flat_map(|t| t.iter_mut().filter_map(|(_, d)| d.as_table_like_mut()))
            .filter(|d| d.contains_key("version"))
            .filter(|d| crate::is_dependency_referred_to_package(*d, &manifest_dir, package_path));

        for dep in deps_to_update {
            let old_req = dep
                .get("version")
                .expect("filter ensures this")
                .as_str()
                .unwrap_or("*");
            if let Some(new_req) = upgrade_requirement(old_req, version)? {
                dep.insert("version", toml_edit::value(new_req));
            }
        }
        local_manifest.write()?;
    }
    Ok(())
}
