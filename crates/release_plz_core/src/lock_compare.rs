use std::collections::{HashMap, HashSet};

use anyhow::Context;
use cargo::{
    core::{
        PackageId, Workspace,
        dependency::{DepKind, Patch},
    },
    util::CanonicalUrl,
};
use cargo_metadata::{Metadata, camino::Utf8Path, semver::Version};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::registry_packages::ReleasedWorkspace;

/// Compare the dependencies present in the `Cargo.lock` of the registry package and the local one.
/// Check if the dependencies of the registry package were updated.
/// This method doesn't detect if the local Cargo.lock added new packages: just if
/// the version of the packages changed.
/// This is enough to understand if the package was updated.
pub fn are_lock_dependencies_updated(
    local_lock: &Utf8Path,
    registry_package: &Utf8Path,
) -> anyhow::Result<bool> {
    let registry_lock = &registry_package.join("Cargo.lock");
    let Some((local_lock, registry_lock)) = read_lockfiles(local_lock, registry_lock)? else {
        return Ok(false);
    };
    Ok(are_dependencies_updated(&local_lock, &registry_lock))
}

/// Compare only dependencies reachable from a package in a historical workspace lockfile.
///
/// The local lockfile is read from disk (the updater reverts it after `cargo package`).
/// The released side is decoded from the lockfile committed at the release.
pub(crate) fn are_workspace_lock_dependencies_updated(
    local_metadata: &Metadata,
    released_workspace: &ReleasedWorkspace,
    package_name: &str,
) -> anyhow::Result<bool> {
    if released_workspace.lockfile.is_none()
        || !local_metadata.workspace_root.join("Cargo.lock").exists()
    {
        return Ok(false);
    }
    let local_package = cargo_utils::workspace_package(local_metadata, package_name)?;
    let local = workspace_lock_dependencies(local_metadata, local_package)?;
    // The user can fix the local lockfile.
    let local = local.with_context(|| {
        format!(
            "cannot find package {package_name:?} {} in lockfile {:?}. Hint: run `cargo check` to update the lockfile and commit it.",
            local_package.version,
            local_metadata.workspace_root.join("Cargo.lock")
        )
    })?;
    released_workspace.restore_lockfile()?;
    let released_package =
        cargo_utils::workspace_package(&released_workspace.metadata, package_name)?;
    let Some(released) =
        workspace_lock_dependencies(&released_workspace.metadata, released_package)?
    else {
        // History can't be rewritten: don't fail, assume the dependencies changed.
        warn!(
            "package {package_name} is not in the Cargo.lock committed at {}: \
             the lockfile was stale when the package was released. \
             Assuming its dependencies changed.",
            released_workspace.commit
        );
        return Ok(true);
    };
    Ok(are_dependencies_updated(&local, &released))
}

/// Collect the dependencies reachable from the workspace `package` in the workspace lockfile.
///
/// Returns `None` when the lockfile is stale, i.e. it doesn't contain the package at
/// the version declared in its manifest.
fn workspace_lock_dependencies(
    metadata: &Metadata,
    package: &cargo_metadata::Package,
) -> anyhow::Result<Option<Lockfile>> {
    let lock_path = metadata.workspace_root.join("Cargo.lock");
    let config = crate::cargo::new_cargo_config(Some(metadata.workspace_root.clone()))?;
    let manifest = metadata.workspace_root.join("Cargo.toml");
    let workspace = Workspace::new(manifest.as_std_path(), &config).with_context(|| {
        format!(
            "cannot load workspace manifest {manifest:?} with the Cargo library bundled in release-plz"
        )
    })?;
    // This only decodes the committed lockfile. It does not resolve dependencies,
    // fetch registry/Git sources, or rewrite the historical workspace.
    let resolve = cargo::ops::load_pkg_lockfile(&workspace)
        .with_context(|| format!("cannot load workspace lockfile {lock_path:?}"))?
        .with_context(|| format!("workspace lockfile {lock_path:?} is missing"))?;
    let patches = workspace
        .root_patch()?
        .into_iter()
        .map(|(url, patches)| Ok((CanonicalUrl::new(&url)?, patches)))
        .collect::<anyhow::Result<HashMap<_, _>>>()?;
    let Some(root) = resolve.iter().find(|id| {
        id.name().as_str() == package.name.as_str()
            && id.version() == &package.version
            && id.source_id().is_path()
    }) else {
        return Ok(None);
    };
    let mut pending = vec![root];
    let mut reachable = HashSet::new();
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let workspace_package = workspace.members().find(|p| p.package_id() == id);
        for (dependency, _) in resolve.deps_not_replaced(id) {
            // A loaded lockfile has no dependency kinds. Workspace manifests tell
            // us which edges belong only to a dependency's tests. Classify the
            // original edge before following `[replace]`, so its declared source
            // is not confused with a dev alias pointing at the replacement.
            if id == root
                || workspace_package
                    .is_none_or(|package| !is_dev_only_dependency(dependency, package, &patches))
            {
                pending.push(resolve.replacement(dependency).unwrap_or(dependency));
            }
        }
    }
    Ok(Some(Lockfile {
        packages: reachable
            .into_iter()
            .map(|id| Package {
                name: id.name().to_string(),
                version: id.version().clone(),
            })
            .collect(),
    }))
}

fn is_dev_only_dependency(
    id: PackageId,
    package: &cargo::core::Package,
    patches: &HashMap<CanonicalUrl, Vec<Patch>>,
) -> bool {
    let mut matching = package
        .dependencies()
        .iter()
        .filter(|dependency| {
            dependency.matches_id(id)
                // A patched normal/build declaration can resolve to the same package
                // as a dev declaration with a directly matching source. Include both.
                || (dependency.matches_ignoring_source(id)
                    && patches
                        .get(dependency.source_id().canonical_url())
                        .is_some_and(|patches| patches.iter().any(|patch| patch.dep.matches_id(id))))
        })
        .peekable();
    matching.peek().is_some() && matching.all(|d| d.kind() == DepKind::Development)
}

fn read_lockfiles(
    local_lock: &Utf8Path,
    registry_lock: &Utf8Path,
) -> anyhow::Result<Option<(Lockfile, Lockfile)>> {
    if !local_lock.exists() || !registry_lock.exists() {
        return Ok(None);
    }
    let local_lock = read_lockfile(local_lock)
        .with_context(|| format!("failed to load lockfile of local package {local_lock:?}"))?;
    let registry_lock = read_lockfile(registry_lock).with_context(|| {
        format!("failed to load lockfile of registry package {registry_lock:?}")
    })?;
    Ok(Some((local_lock, registry_lock)))
}

fn read_lockfile(path: &Utf8Path) -> anyhow::Result<Lockfile> {
    let content = fs_err::read_to_string(path).context("can't read lockfile")?;
    let lockfile =
        toml::from_str(&content).with_context(|| format!("invalid format of lockfile {path:?}"))?;
    Ok(lockfile)
}

fn are_dependencies_updated(local_lock: &Lockfile, released_lock: &Lockfile) -> bool {
    let mut local_versions: HashMap<&str, HashSet<&Version>> = HashMap::new();
    for package in &local_lock.packages {
        local_versions
            .entry(&package.name)
            .or_default()
            .insert(&package.version);
    }
    // The local lockfile can contain extra dev dependencies. Preserve the registry
    // comparison's behavior: only changes to previously released versions count.
    released_lock.packages.iter().any(|package| {
        let changed = local_versions
            .get(package.name.as_str())
            .is_some_and(|versions| !versions.contains(&package.version));
        if changed {
            debug!(
                "Version of package {} changed from {}",
                package.name, package.version
            );
        }
        changed
    })
}

#[derive(Deserialize, Debug)]
struct Lockfile {
    #[serde(rename = "package")]
    packages: Vec<Package>,
}

#[derive(Deserialize, Debug)]
struct Package {
    name: String,
    version: Version,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{generate_lockfile, package_manifest, run_cargo_ok, write_package};
    use cargo_metadata::DependencyKind;

    /// Spell a source the way Cargo writes it into a lockfile: `encodable_source_id`
    /// percent-encodes the Git query parameters, while `cargo metadata` leaves them decoded.
    fn encoded_source(source: &str) -> String {
        match source.split_once('?') {
            Some((base, query)) => {
                format!("{base}?{}", query.replace('+', "%2B").replace('/', "%2F"))
            }
            None => source.to_owned(),
        }
    }

    /// Give hand-written lockfile fixtures real path packages for Cargo's decoder.
    /// The lockfile entries reachable from `package`, which must be in the lockfile.
    fn lock_dependencies(metadata: &Metadata, package: &str) -> Lockfile {
        workspace_lock_dependencies(
            metadata,
            cargo_utils::workspace_package(metadata, package).unwrap(),
        )
        .unwrap()
        .unwrap()
    }

    fn workspace_for_lock(lockfile: &str) -> (crate::fs_utils::Utf8TempDir, Metadata) {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let value: toml::Value = toml::from_str(lockfile).unwrap();
        let mut members = Vec::new();
        for package in value["package"].as_array().unwrap() {
            if package.get("source").is_some() {
                continue;
            }
            let name = package["name"].as_str().unwrap();
            let version = package["version"].as_str().unwrap();
            let member = format!("{name}-{version}");
            write_package(&directory.path().join(&member), name, version, "");
            members.push(member);
        }
        let manifest = directory.path().join("Cargo.toml");
        fs_err::write(
            &manifest,
            format!("[workspace]\nmembers = {members:?}\nresolver = \"2\"\n"),
        )
        .unwrap();
        fs_err::write(directory.path().join("Cargo.lock"), lockfile).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        (directory, metadata)
    }

    /// Snapshot the lockfile of a workspace the way the reconstruction does.
    fn released_workspace(metadata: Metadata) -> ReleasedWorkspace {
        ReleasedWorkspace::new(metadata, "release-commit".into()).unwrap()
    }

    fn compare_workspace_locks(local: &Utf8Path, released: &Utf8Path, package: &str) -> bool {
        let (_local, local_metadata) = workspace_for_lock(&fs_err::read_to_string(local).unwrap());
        let (_released, released_metadata) =
            workspace_for_lock(&fs_err::read_to_string(released).unwrap());
        let released = released_workspace(released_metadata);
        are_workspace_lock_dependencies_updated(&local_metadata, &released, package).unwrap()
    }

    /// A lockfile in which `app 0.1.0` depends on `dep 1.0.0`.
    const APP_DEP_LOCKFILE: &str = "version = 4\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"dep\"]\n\
             [[package]]\nname = \"dep\"\nversion = \"1.0.0\"\n";

    /// A workspace whose manifest version (`app 0.2.0`) is ahead of the committed
    /// lockfile, which still records `app 0.1.0`.
    fn stale_workspace() -> (crate::fs_utils::Utf8TempDir, Metadata) {
        let (directory, _) = workspace_for_lock(APP_DEP_LOCKFILE);
        let manifest = directory.path().join("app-0.1.0/Cargo.toml");
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(&manifest, contents.replace("0.1.0", "0.2.0")).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        (directory, metadata)
    }

    #[test]
    fn workspace_lock_comparison_fails_on_stale_lockfile() {
        let (directory, local_metadata) = stale_workspace();
        let (_released, released_metadata) = workspace_for_lock(APP_DEP_LOCKFILE);
        let released = released_workspace(released_metadata);
        // The user can fix the local lockfile, so this is an error with a hint.
        let error = are_workspace_lock_dependencies_updated(&local_metadata, &released, "app")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("cannot find package \"app\" 0.2.0"),
            "{error}"
        );
        assert!(error.contains("cargo check"), "{error}");
        assert_eq!(
            fs_err::read_to_string(directory.path().join("Cargo.lock")).unwrap(),
            APP_DEP_LOCKFILE
        );
    }

    #[test]
    fn workspace_lock_comparison_treats_stale_released_lockfile_as_updated() {
        let (_local, local_metadata) = workspace_for_lock(APP_DEP_LOCKFILE);
        let (_released, released_metadata) = stale_workspace();
        let released = released_workspace(released_metadata);
        // History can't be rewritten: assume the dependencies changed instead of failing.
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released, "app").unwrap()
        );
    }

    #[test]
    fn workspace_lock_comparison_uses_lockfile_committed_at_release() {
        let committed = APP_DEP_LOCKFILE;
        let re_resolved = committed.replace("1.0.0", "1.0.1");
        let (_local, local_metadata) = workspace_for_lock(&re_resolved);
        let (released_dir, released_metadata) = workspace_for_lock(committed);
        let released = released_workspace(released_metadata);
        assert_eq!(released.lockfile.as_deref(), Some(committed));
        // Simulate `cargo package --list` re-resolving the released lockfile on disk
        // after the snapshot was taken.
        let released_lock = released_dir.path().join("Cargo.lock");
        fs_err::write(&released_lock, &re_resolved).unwrap();
        // Comparing against the re-resolved lockfile would report no change.
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released, "app").unwrap()
        );
        assert_eq!(fs_err::read_to_string(&released_lock).unwrap(), committed);

        // Without a lockfile at the release there is nothing to compare.
        let released = ReleasedWorkspace {
            lockfile: None,
            ..released
        };
        assert!(
            !are_workspace_lock_dependencies_updated(&local_metadata, &released, "app").unwrap()
        );
    }

    #[test]
    fn workspace_lock_comparison_distinguishes_same_named_path_packages() {
        let local = crate::fs_utils::Utf8TempDir::new().unwrap();
        let released = crate::fs_utils::Utf8TempDir::new().unwrap();
        for (directory, dependency_version) in [(local.path(), "1.0.1"), (released.path(), "1.0.0")]
        {
            fs_err::write(
                directory.join("Cargo.toml"),
                "[workspace]\nmembers = [\"app\", \"other\"]\nexclude = [\"older-app\"]\nresolver = \"2\"\n",
            )
            .unwrap();
            for (path, name, version, dependencies) in [
                ("app", "app", "0.1.0", ""),
                (
                    "other",
                    "other",
                    "0.1.0",
                    "app = { path = \"../older-app\", version = \"1\" }",
                ),
                ("older-app", "app", dependency_version, ""),
            ] {
                write_package(
                    &directory.join(path),
                    name,
                    version,
                    &format!("[dependencies]\n{dependencies}\n"),
                );
            }
            generate_lockfile(directory);
        }
        let local_metadata =
            cargo_utils::get_manifest_metadata(&local.path().join("Cargo.toml")).unwrap();
        let released = released_workspace(
            cargo_utils::get_manifest_metadata(&released.path().join("Cargo.toml")).unwrap(),
        );
        assert!(
            !are_workspace_lock_dependencies_updated(&local_metadata, &released, "app").unwrap()
        );
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released, "other").unwrap()
        );

        // Each side must use its own member version when identifying the root.
        let manifest = local.path().join("app/Cargo.toml");
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(&manifest, contents.replace("0.1.0", "0.1.1")).unwrap();
        generate_lockfile(local.path());
        let local_metadata =
            cargo_utils::get_manifest_metadata(&local.path().join("Cargo.toml")).unwrap();
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released, "app").unwrap()
        );
    }

    #[test]
    fn workspace_lock_comparison_distinguishes_path_and_git_packages() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let git_path = directory.path().join("dependency");
        let workspace = directory.path().join("workspace");
        fs_err::create_dir(&git_path).unwrap();
        let git_repo = git_cmd::Repo::init(&git_path);
        let git_url = url::Url::from_directory_path(&git_path).unwrap();
        for (root, members) in [
            (&git_path, &["shared", "leaf"][..]),
            (&workspace, &["app", "other", "shared"][..]),
        ] {
            fs_err::create_dir_all(root).unwrap();
            fs_err::write(
                root.join("Cargo.toml"),
                format!("[workspace]\nmembers = {members:?}\nresolver = \"2\"\n"),
            )
            .unwrap();
            for name in members {
                let dependencies = match *name {
                    "shared" if root == &git_path => "leaf = { path = \"../leaf\" }".into(),
                    "app" => "shared = { path = \"../shared\" }".into(),
                    "other" => format!("shared = {{ git = {:?} }}", git_url.as_str()),
                    _ => String::new(),
                };
                write_package(
                    &root.join(name),
                    name,
                    "1.0.0",
                    &format!("[dependencies]\n{dependencies}\n"),
                );
            }
        }
        git_repo.add_all_and_commit("initial dependency").unwrap();
        // Let Cargo encode the ambiguous path/Git dependency IDs itself.
        run_cargo_ok(&workspace, &["generate-lockfile"]);
        let released_lock = directory.path().join("released.lock");
        let local_lock = workspace.join("Cargo.lock");
        fs_err::copy(&local_lock, &released_lock).unwrap();
        assert!(!compare_workspace_locks(&local_lock, &released_lock, "app"));
        assert!(!compare_workspace_locks(
            &local_lock,
            &released_lock,
            "other"
        ));

        let leaf_manifest = git_path.join("leaf/Cargo.toml");
        let manifest = fs_err::read_to_string(&leaf_manifest).unwrap();
        fs_err::write(&leaf_manifest, manifest.replace("1.0.0", "1.0.1")).unwrap();
        git_repo.add_all_and_commit("update Git leaf").unwrap();
        run_cargo_ok(&workspace, &["update"]);

        // Only `other` reaches the Git package and its updated transitive dependency.
        assert!(!compare_workspace_locks(&local_lock, &released_lock, "app"));
        assert!(compare_workspace_locks(
            &local_lock,
            &released_lock,
            "other"
        ));
    }

    #[test]
    fn workspace_lock_comparison_ignores_transitive_dev_dependencies() {
        let local = crate::fs_utils::Utf8TempDir::new().unwrap();
        let released = crate::fs_utils::Utf8TempDir::new().unwrap();
        for directory in [local.path(), released.path()] {
            fs_err::write(
                directory.join("Cargo.toml"),
                "[workspace]\nmembers = [\"binary\", \"library\"]\nresolver = \"2\"\n\
                 [patch.crates-io]\npatched-test = { git = \"https://example.com/patched-test\" }\n",
            )
            .unwrap();
            for (name, dependencies) in [
                (
                    "binary",
                    r#"
[dependencies]
library = { path = "../library" }
[dev-dependencies]
root-dev = "1"
"#,
                ),
                (
                    "library",
                    r#"
[dependencies]
shared = "1"
both = "1"
[build-dependencies]
builder = "1"
[dev-dependencies]
test-only = "1"
patched-test = "1"
both = "1"
shared-test = { package = "shared", version = "2" }
"#,
                ),
            ] {
                write_package(&directory.join(name), name, "0.1.0", dependencies);
            }
        }
        // No dependency resolution or registry access is needed to read dependency kinds.
        let local_metadata =
            cargo_utils::get_manifest_metadata(&local.path().join("Cargo.toml")).unwrap();
        let released_metadata =
            cargo_utils::get_manifest_metadata(&released.path().join("Cargo.toml")).unwrap();
        let mut lockfile = r#"
version = 4
[[package]]
name = "binary"
version = "0.1.0"
dependencies = ["library", "root-dev"]
[[package]]
name = "library"
version = "0.1.0"
dependencies = ["shared 1.0.0", "shared 2.0.0", "both", "builder", "test-only", "patched-test"]
[[package]]
name = "patched-test"
version = "1.0.0"
source = "git+https://example.com/patched-test#0123456789abcdef"
"#
        .to_string();
        for (name, version) in [
            ("shared", "1.0.0"),
            ("shared", "2.0.0"),
            ("both", "1.0.0"),
            ("builder", "1.0.0"),
            ("test-only", "1.0.0"),
            ("root-dev", "1.0.0"),
        ] {
            lockfile.push_str(&format!(
                "\n[[package]]\nname = {name:?}\nversion = {version:?}\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
            ));
        }
        fs_err::write(released.path().join("Cargo.lock"), &lockfile).unwrap();
        let released = released_workspace(released_metadata);

        for (name, version, should_update) in [
            ("test-only", "1.0.0", false),
            // A registry dev dependency can resolve to a different source via [patch].
            ("patched-test", "1.0.0", false),
            ("shared", "2.0.0", false),
            ("shared", "1.0.0", true),
            ("both", "1.0.0", true),
            ("builder", "1.0.0", true),
            ("root-dev", "1.0.0", true),
        ] {
            let mut updated_version = Version::parse(version).unwrap();
            updated_version.patch += 1;
            let changed = lockfile
                .replace(
                    &format!("name = {name:?}\nversion = {version:?}"),
                    &format!("name = {name:?}\nversion = \"{updated_version}\""),
                )
                .replace(
                    &format!("{name} {version}"),
                    &format!("{name} {updated_version}"),
                );
            fs_err::write(local.path().join("Cargo.lock"), changed).unwrap();
            assert_eq!(
                are_workspace_lock_dependencies_updated(&local_metadata, &released, "binary")
                    .unwrap(),
                should_update,
                "updating {name} {version}"
            );
        }
    }

    #[test]
    fn workspace_lock_comparison_preserves_patched_path_dependencies() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let root = directory.path();
        fs_err::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"binary\", \"library\", \"shared\", \"normal-leaf\", \"dev-leaf\"]\n\
             exclude = [\"test-shared\"]\nresolver = \"2\"\n\
             [patch.crates-io]\nshared = { path = \"shared\" }\n",
        )
        .unwrap();
        for (path, name, version, dependencies) in [
            (
                "binary",
                "binary",
                "0.1.0",
                "[dependencies]\nlibrary = { path = \"../library\" }\n",
            ),
            (
                "library",
                "library",
                "0.1.0",
                "[dependencies]\nshared = \"1.0\"\n\
                 [dev-dependencies]\nshared-test = { package = \"shared\", path = \"../test-shared\" }\n",
            ),
            (
                "shared",
                "shared",
                "1.0.0",
                "[dependencies]\nnormal-leaf = { path = \"../normal-leaf\" }\n",
            ),
            (
                "test-shared",
                "shared",
                "2.0.0",
                "[workspace]\n[dependencies]\ndev-leaf = { path = \"../dev-leaf\" }\n",
            ),
            ("normal-leaf", "normal-leaf", "1.0.0", ""),
            ("dev-leaf", "dev-leaf", "1.0.0", ""),
        ] {
            write_package(&root.join(path), name, version, dependencies);
        }
        let read_dependencies = || {
            generate_lockfile(root);
            let metadata = cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
            lock_dependencies(&metadata, "binary")
        };
        for dependency_kind in ["dependencies", "build-dependencies"] {
            // A dev declaration may resolve to the patched runtime package itself,
            // or to a distinct package with the same name. Preserve only runtime edges.
            for dev_path in ["shared", "test-shared"] {
                fs_err::write(
                    root.join("library/Cargo.toml"),
                    package_manifest(
                        "library",
                        "0.1.0",
                        &format!(
                            "[{dependency_kind}]\nshared = \"1.0\"\n\
                             [dev-dependencies]\nshared-test = {{ package = \"shared\", path = \"../{dev_path}\" }}\n"
                        ),
                    ),
                )
                .unwrap();
                let released = read_dependencies();
                for (leaf, should_update) in [("normal-leaf", true), ("dev-leaf", false)] {
                    let manifest = root.join(leaf).join("Cargo.toml");
                    let original = fs_err::read_to_string(&manifest).unwrap();
                    fs_err::write(&manifest, original.replace("1.0.0", "1.0.1")).unwrap();
                    let local = read_dependencies();
                    assert_eq!(
                        are_dependencies_updated(&local, &released),
                        should_update,
                        "updating {leaf} with {dependency_kind} and dev path {dev_path}"
                    );
                    fs_err::write(manifest, original).unwrap();
                }
            }
        }
    }

    #[test]
    fn workspace_lock_comparison_preserves_replaced_path_dependencies() {
        // Cargo records the original registry edge and its replacement separately.
        let lockfile = r#"
version = 4
[[package]]
name = "app"
version = "1.0.0"
dependencies = ["library"]
[[package]]
name = "library"
version = "1.0.0"
dependencies = ["shared 1.0.0", "shared 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"]
[[package]]
name = "shared"
version = "1.0.0"
dependencies = ["leaf"]
[[package]]
name = "shared"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
replace = "shared 1.0.0"
[[package]]
name = "leaf"
version = "1.0.0"
"#;
        let updated_lockfile = lockfile.replace(
            "name = \"leaf\"\nversion = \"1.0.0\"",
            "name = \"leaf\"\nversion = \"1.0.1\"",
        );
        for (kind, should_update) in [
            ("dependencies", true),
            ("build-dependencies", true),
            ("dev-dependencies", false),
        ] {
            let read_dependencies = |lockfile: &str| {
                let (directory, metadata) = workspace_for_lock(lockfile);
                let root = directory.path();
                let leaf = cargo_utils::workspace_package(&metadata, "leaf").unwrap();
                let dev_table = if kind == "dev-dependencies" {
                    ""
                } else {
                    "[dev-dependencies]\n"
                };
                for (manifest, declarations) in [
                    (
                        "Cargo.toml",
                        "[replace]\n\"shared:1.0.0\" = { path = \"shared-1.0.0\" }\n".to_owned(),
                    ),
                    (
                        "app-1.0.0/Cargo.toml",
                        "[dependencies]\nlibrary = { path = \"../library-1.0.0\" }\n".to_owned(),
                    ),
                    (
                        "library-1.0.0/Cargo.toml",
                        format!(
                            "[{kind}]\nshared = \"=1.0.0\"\n\
                             {dev_table}shared-test = {{ package = \"shared\", path = \"../shared-1.0.0\" }}\n"
                        ),
                    ),
                    (
                        "shared-1.0.0/Cargo.toml",
                        format!(
                            "[dependencies]\nleaf = {{ path = \"../leaf-{}\" }}\n",
                            leaf.version
                        ),
                    ),
                ] {
                    let manifest = root.join(manifest);
                    let contents = fs_err::read_to_string(&manifest).unwrap();
                    fs_err::write(manifest, format!("{contents}{declarations}")).unwrap();
                }
                let metadata =
                    cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
                lock_dependencies(&metadata, "app")
            };
            let released = read_dependencies(lockfile);
            let local = read_dependencies(&updated_lockfile);
            assert_eq!(
                are_dependencies_updated(&local, &released),
                should_update,
                "updating a replacement's dependency through {kind}"
            );
        }
    }

    #[test]
    fn workspace_lock_comparison_ignores_git_dev_dependency_with_plus_in_branch() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let git_path = directory.path().join("git-dependency");
        fs_err::create_dir_all(&git_path).unwrap();
        let git_repo = git_cmd::Repo::init(&git_path);
        write_package(
            &git_path,
            "shared",
            "1.0.0",
            "[dependencies]\ndev-leaf = { path = \"leaf\" }\n",
        );
        let leaf_manifest = git_path.join("leaf/Cargo.toml");
        let leaf = package_manifest("dev-leaf", "1.0.0", "");
        write_package(&git_path.join("leaf"), "dev-leaf", "1.0.0", "");
        git_repo.add_all_and_commit("initial dependency").unwrap();
        git_cmd::git_in_dir(&git_path, &["checkout", "-b", "feature+next"]).unwrap();
        let git_url = url::Url::from_directory_path(&git_path).unwrap();

        let local = directory.path().join("local");
        let released = directory.path().join("released");
        for root in [&local, &released] {
            fs_err::create_dir_all(root).unwrap();
            fs_err::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers = [\"binary\", \"library\", \"shared\"]\nresolver = \"2\"\n",
            )
            .unwrap();
            for (name, dependencies) in [
                (
                    "binary",
                    "[dependencies]\nlibrary = { path = \"../library\" }\n".to_owned(),
                ),
                (
                    "library",
                    format!(
                        "[dependencies]\nshared = {{ path = \"../shared\" }}\n[dev-dependencies]\nshared-test = {{ package = \"shared\", git = {:?}, branch = \"feature+next\" }}\n",
                        git_url.as_str()
                    ),
                ),
                ("shared", String::new()),
            ] {
                write_package(&root.join(name), name, "1.0.0", &dependencies);
            }
        }
        run_cargo_ok(&local, &["generate-lockfile"]);
        fs_err::copy(local.join("Cargo.lock"), released.join("Cargo.lock")).unwrap();
        fs_err::write(&leaf_manifest, leaf.replace("1.0.0", "1.0.1")).unwrap();
        git_repo
            .add_all_and_commit("update dev dependency")
            .unwrap();
        run_cargo_ok(&local, &["update"]);

        let local_metadata = cargo_utils::get_manifest_metadata(&local.join("Cargo.toml")).unwrap();
        let released_metadata =
            cargo_utils::get_manifest_metadata(&released.join("Cargo.toml")).unwrap();
        assert!(
            fs_err::read_to_string(local.join("Cargo.lock"))
                .unwrap()
                .contains("feature%2Bnext")
        );
        let released = released_workspace(released_metadata);
        assert!(
            !are_workspace_lock_dependencies_updated(&local_metadata, &released, "binary").unwrap()
        );
        // The same update must still count for the package declaring the dev dependency.
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released, "library").unwrap()
        );
    }

    #[test]
    fn workspace_lock_comparison_distinguishes_dependency_sources() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        fs_err::write(
            directory.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"binary\", \"library\", \"shared\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        for name in ["binary", "library", "shared"] {
            write_package(&directory.path().join(name), name, "1.0.0", "");
        }
        fs_err::write(
            directory.path().join("binary/Cargo.toml"),
            package_manifest(
                "binary",
                "1.0.0",
                "[dependencies]\nlibrary = { path = \"../library\" }\n",
            ),
        )
        .unwrap();
        let path = "path = \"../shared\"";
        let registry = "version = \"1\"";
        let git = "git = \"https://example.com/shared\"";
        let other_git = "git = \"https://example.com/other\"";
        let branch = "git = \"https://example.com/shared\", branch = \"next\"";
        // A branch name that Cargo percent-encodes in the lockfile but not in metadata.
        let escaped_branch = "git = \"https://example.com/shared\", branch = \"feature/next\"";
        let plus_branch = "git = \"https://example.com/shared\", branch = \"feature+next\"";
        for (normal, dev) in [
            (git, other_git),
            (git, branch),
            (git, escaped_branch),
            (escaped_branch, git),
            (escaped_branch, branch),
            (git, plus_branch),
            (plus_branch, git),
            (registry, git),
            (git, registry),
            (path, git),
            (git, path),
            (path, registry),
            (registry, path),
        ] {
            fs_err::write(
                directory.path().join("library/Cargo.toml"),
                package_manifest(
                    "library",
                    "1.0.0",
                    &format!(
                        "[dependencies]\nshared = {{ {normal} }}\n\
                         [dev-dependencies]\nshared-test = {{ package = \"shared\", {dev} }}\n"
                    ),
                ),
            )
            .unwrap();
            // Read real dependency declarations without resolving or fetching them.
            let metadata =
                cargo_utils::get_manifest_metadata(&directory.path().join("Cargo.toml")).unwrap();
            let library = metadata
                .packages
                .iter()
                .find(|p| p.name == "library")
                .unwrap();
            let mut dependency_ids = Vec::new();
            let mut lockfile = String::from(
                "version = 4\n[[package]]\nname = \"binary\"\nversion = \"1.0.0\"\ndependencies = [\"library\"]\n",
            );
            for dependency in &library.dependencies {
                let leaf = if dependency.kind == DependencyKind::Development {
                    "dev-leaf"
                } else {
                    "normal-leaf"
                };
                let source = dependency.source.as_ref().map(|s| s.repr.as_str());
                // Cargo writes dependency IDs with the same encoding it uses for the
                // package's own `source`, minus the resolved revision.
                dependency_ids.push(source.map_or_else(
                    || "shared 1.0.0".to_string(),
                    |source| format!("shared 1.0.0 ({})", encoded_source(source)),
                ));
                lockfile.push_str("[[package]]\nname = \"shared\"\nversion = \"1.0.0\"\n");
                if let Some(source) = source {
                    let precise = if source.starts_with("git+") {
                        "#0123456789abcdef"
                    } else {
                        ""
                    };
                    lockfile.push_str(&format!(
                        "source = \"{}{precise}\"\n",
                        encoded_source(source)
                    ));
                }
                lockfile.push_str(&format!(
                    "dependencies = [{leaf:?}]\n[[package]]\nname = {leaf:?}\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
                ));
            }
            lockfile.push_str(&format!(
                "[[package]]\nname = \"library\"\nversion = \"1.0.0\"\ndependencies = {dependency_ids:?}\n"
            ));
            fs_err::write(directory.path().join("Cargo.lock"), &lockfile).unwrap();
            let released = lock_dependencies(&metadata, "binary");
            for (leaf, should_update) in [("dev-leaf", false), ("normal-leaf", true)] {
                let changed = lockfile.replace(
                    &format!("name = {leaf:?}\nversion = \"1.0.0\""),
                    &format!("name = {leaf:?}\nversion = \"1.0.1\""),
                );
                fs_err::write(directory.path().join("Cargo.lock"), changed).unwrap();
                let local = lock_dependencies(&metadata, "binary");
                assert_eq!(
                    are_dependencies_updated(&local, &released),
                    should_update,
                    "updating {leaf} with normal {normal} and dev {dev}"
                );
            }
        }
    }

    #[test]
    fn workspace_lock_comparison_follows_source_qualified_git_dependencies() {
        let directory = tempfile::tempdir().unwrap();
        let directory = Utf8Path::from_path(directory.path()).unwrap();
        let released = directory.join("released.lock");
        let local = directory.join("local.lock");
        // Cargo adds sources to dependency IDs when name and version are ambiguous,
        // but only the package source records the precise Git revision.
        let lockfile = r#"
version = 4
[[package]]
name = "binary"
version = "0.1.0"
dependencies = ["shared 0.1.0 (git+https://example.com/one)", "shared 0.1.0 (git+https://example.com/two?branch=next)"]
[[package]]
name = "shared"
version = "0.1.0"
source = "git+https://example.com/one#1111111111111111111111111111111111111111"
dependencies = ["leaf"]
[[package]]
name = "shared"
version = "0.1.0"
source = "git+https://example.com/two?branch=next#2222222222222222222222222222222222222222"
[[package]]
name = "leaf"
version = "1.0.0"
source = "git+https://example.com/one#1111111111111111111111111111111111111111"
"#;
        fs_err::write(&released, lockfile).unwrap();
        fs_err::write(&local, lockfile).unwrap();
        assert!(!compare_workspace_locks(&local, &released, "binary"));
        fs_err::write(&local, lockfile.replace("1.0.0", "1.0.1")).unwrap();
        assert!(compare_workspace_locks(&local, &released, "binary"));
    }

    #[test]
    fn workspace_lock_comparison_ignores_unrelated_packages() {
        let directory = tempfile::tempdir().unwrap();
        let directory = Utf8Path::from_path(directory.path()).unwrap();
        let released = directory.join("released.lock");
        let local = directory.join("local.lock");
        let lockfile = r#"
version = 4
[[package]]
name = "binary"
version = "0.1.0"
dependencies = ["library"]
[[package]]
name = "library"
version = "0.1.0"
dependencies = ["shared 1.0.0 (registry+https://example.com/index)"]
[[package]]
name = "shared"
version = "1.0.0"
source = "registry+https://example.com/index"
[[package]]
name = "shared"
version = "2.0.0"
source = "registry+https://example.com/index"
[[package]]
name = "unrelated"
version = "0.1.0"
dependencies = ["shared 2.0.0"]
"#;
        fs_err::write(&released, lockfile).unwrap();
        fs_err::write(
            &local,
            lockfile.replace("2.0.0", "2.0.1").replace(
                "name = \"unrelated\"\nversion = \"0.1.0\"",
                "name = \"unrelated\"\nversion = \"0.1.1\"",
            ),
        )
        .unwrap();
        assert!(!compare_workspace_locks(&local, &released, "binary"));
        assert!(compare_workspace_locks(&local, &released, "unrelated"));

        fs_err::write(&local, lockfile.replace("1.0.0", "1.0.1")).unwrap();
        assert!(compare_workspace_locks(&local, &released, "binary"));
        assert!(!compare_workspace_locks(&local, &released, "unrelated"));
    }
}
