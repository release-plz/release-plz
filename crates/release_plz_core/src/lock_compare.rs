use std::collections::{HashMap, HashSet};

use anyhow::Context;
use cargo::core::{PackageId, Workspace};
use cargo_metadata::{DependencyKind, Metadata, camino::Utf8Path, semver::Version};
use serde::Deserialize;
use tracing::debug;

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
pub(crate) fn are_workspace_lock_dependencies_updated(
    local_metadata: &Metadata,
    released_metadata: &Metadata,
    package_name: &str,
) -> anyhow::Result<bool> {
    if !local_metadata.workspace_root.join("Cargo.lock").exists()
        || !released_metadata.workspace_root.join("Cargo.lock").exists()
    {
        return Ok(false);
    }
    let local = workspace_lock_dependencies(local_metadata, package_name)?;
    let released = workspace_lock_dependencies(released_metadata, package_name)?;
    Ok(are_dependencies_updated(&local, &released))
}

fn workspace_lock_dependencies(
    metadata: &Metadata,
    package_name: &str,
) -> anyhow::Result<Lockfile> {
    let lock_path = metadata.workspace_root.join("Cargo.lock");
    let config = crate::cargo::new_cargo_config(Some(metadata.workspace_root.clone()))?;
    let manifest = metadata.workspace_root.join("Cargo.toml");
    let workspace = Workspace::new(manifest.as_std_path(), &config)?;
    // This only decodes the committed lockfile. It does not resolve dependencies,
    // fetch registry/Git sources, or rewrite the historical workspace.
    let resolve = cargo::ops::load_pkg_lockfile(&workspace)
        .with_context(|| format!("cannot load workspace lockfile {lock_path:?}"))?
        .with_context(|| format!("workspace lockfile {lock_path:?} is missing"))?;
    let package = cargo_utils::workspace_package(metadata, package_name)?;
    let root = resolve
        .iter()
        .find(|id| id.name().as_str() == package_name
            && id.version() == &package.version
            && id.source_id().is_path())
        .with_context(|| format!(
            "cannot find package {package_name:?} {} in lockfile {lock_path:?}. Hint: run `cargo check` to update the lockfile and commit it.",
            package.version
        ))?;
    let mut pending = vec![root];
    let mut reachable = HashSet::new();
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let workspace_package = metadata.packages.iter().find(|p| {
            id.source_id().is_path()
                && p.name.as_str() == id.name().as_str()
                && &p.version == id.version()
        });
        for (dependency, _) in resolve.deps(id) {
            // A loaded lockfile has no dependency kinds. Workspace metadata tells
            // us which edges belong only to a dependency's tests.
            if id == root
                || workspace_package
                    .is_none_or(|package| !is_dev_only_dependency(dependency, package))
            {
                pending.push(dependency);
            }
        }
    }
    Ok(Lockfile {
        packages: reachable
            .into_iter()
            .map(|id| Package {
                name: id.name().to_string(),
                version: id.version().clone(),
            })
            .collect(),
    })
}

fn is_dev_only_dependency(id: PackageId, package: &cargo_metadata::Package) -> bool {
    let matching = package.dependencies.iter().filter(|dependency| {
        dependency.name == id.name().as_str()
            && (dependency.req == cargo_metadata::semver::VersionReq::STAR
                || dependency.req.matches(id.version()))
    });
    // Cargo decodes the lockfile's source and renders the same unescaped spelling
    // used by metadata, preserving literal '+' and '%' in Git references.
    let path = id.source_id().local_path();
    let source = path
        .is_none()
        .then(|| id.source_id().without_precise().as_url().to_string());
    let matches_source = |dependency: &cargo_metadata::Dependency| {
        if let Some(path) = &path {
            // Path sources all have `source = None` in metadata. Distinguish their
            // directories so an unrelated dev dependency cannot hide a patched one.
            dependency
                .path
                .as_ref()
                .is_some_and(|dependency_path| dependency_path.as_std_path() == path)
        } else {
            source.as_deref()
                == dependency
                    .source
                    .as_ref()
                    .map(|source| source.repr.as_str())
        }
    };
    // A patch can replace the declared source. If no source matches, only
    // discard the dependency when every matching declaration is dev-only.
    let has_source_match = matching.clone().any(matches_source);
    let mut matching = matching
        .filter(|d| !has_source_match || matches_source(d))
        .peekable();
    matching.peek().is_some() && matching.all(|d| d.kind == DependencyKind::Development)
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
            let path = directory.path().join(&member);
            fs_err::create_dir_all(path.join("src")).unwrap();
            fs_err::write(path.join("src/lib.rs"), "").unwrap();
            fs_err::write(
                path.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nversion = {version:?}\nedition = \"2021\"\n"),
            )
            .unwrap();
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

    fn compare_workspace_locks(local: &Utf8Path, released: &Utf8Path, package: &str) -> bool {
        let (_local, local_metadata) = workspace_for_lock(&fs_err::read_to_string(local).unwrap());
        let (_released, released_metadata) =
            workspace_for_lock(&fs_err::read_to_string(released).unwrap());
        are_workspace_lock_dependencies_updated(&local_metadata, &released_metadata, package)
            .unwrap()
    }

    #[test]
    fn workspace_lock_comparison_fails_on_stale_lockfile() {
        let lockfile = "version = 4\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"dep\"]\n\
             [[package]]\nname = \"dep\"\nversion = \"1.0.0\"\n";
        let (directory, _) = workspace_for_lock(lockfile);
        let manifest = directory.path().join("app-0.1.0/Cargo.toml");
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(&manifest, contents.replace("0.1.0", "0.2.0")).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        let error = workspace_lock_dependencies(&metadata, "app")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("cannot find package \"app\" 0.2.0"),
            "{error}"
        );
        assert_eq!(
            fs_err::read_to_string(directory.path().join("Cargo.lock")).unwrap(),
            lockfile
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
                let package = directory.join(path);
                fs_err::create_dir_all(package.join("src")).unwrap();
                fs_err::write(package.join("src/lib.rs"), "").unwrap();
                fs_err::write(
                    package.join("Cargo.toml"),
                    format!("[package]\nname = {name:?}\nversion = {version:?}\nedition = \"2021\"\n[dependencies]\n{dependencies}\n"),
                )
                .unwrap();
            }
            let output =
                crate::cargo::run_cargo(directory, &["generate-lockfile", "--offline"]).unwrap();
            assert!(output.status.success(), "{}", output.stderr);
        }
        let local_metadata =
            cargo_utils::get_manifest_metadata(&local.path().join("Cargo.toml")).unwrap();
        let released_metadata =
            cargo_utils::get_manifest_metadata(&released.path().join("Cargo.toml")).unwrap();
        assert!(
            !are_workspace_lock_dependencies_updated(&local_metadata, &released_metadata, "app")
                .unwrap()
        );
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released_metadata, "other")
                .unwrap()
        );

        // Each side must use its own member version when identifying the root.
        let manifest = local.path().join("app/Cargo.toml");
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(&manifest, contents.replace("0.1.0", "0.1.1")).unwrap();
        let output =
            crate::cargo::run_cargo(local.path(), &["generate-lockfile", "--offline"]).unwrap();
        assert!(output.status.success(), "{}", output.stderr);
        let local_metadata =
            cargo_utils::get_manifest_metadata(&local.path().join("Cargo.toml")).unwrap();
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released_metadata, "app")
                .unwrap()
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
                let package = root.join(name);
                fs_err::create_dir_all(package.join("src")).unwrap();
                fs_err::write(package.join("src/lib.rs"), "").unwrap();
                let dependencies = match *name {
                    "shared" if root == &git_path => "leaf = { path = \"../leaf\" }".into(),
                    "app" => "shared = { path = \"../shared\" }".into(),
                    "other" => format!("shared = {{ git = {:?} }}", git_url.as_str()),
                    _ => String::new(),
                };
                fs_err::write(
                    package.join("Cargo.toml"),
                    format!(
                        "[package]\nname = {name:?}\nversion = \"1.0.0\"\nedition = \"2021\"\n\
                         [dependencies]\n{dependencies}\n"
                    ),
                )
                .unwrap();
            }
        }
        git_repo.add_all_and_commit("initial dependency").unwrap();
        let run_cargo = |args: &[&str]| {
            let output = crate::cargo::run_cargo(&workspace, args).unwrap();
            assert!(output.status.success(), "{}", output.stderr);
        };
        // Let Cargo encode the ambiguous path/Git dependency IDs itself.
        run_cargo(&["generate-lockfile"]);
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
        run_cargo(&["update"]);

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
                let package = directory.join(name);
                fs_err::create_dir_all(package.join("src")).unwrap();
                fs_err::write(package.join("src/lib.rs"), "").unwrap();
                fs_err::write(
                    package.join("Cargo.toml"),
                    format!(
                        "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2021\"\n{dependencies}"
                    ),
                )
                .unwrap();
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
                are_workspace_lock_dependencies_updated(
                    &local_metadata,
                    &released_metadata,
                    "binary"
                )
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
            let package = root.join(path);
            fs_err::create_dir_all(package.join("src")).unwrap();
            fs_err::write(package.join("src/lib.rs"), "").unwrap();
            fs_err::write(
                package.join("Cargo.toml"),
                format!(
                    "[package]\nname = {name:?}\nversion = {version:?}\nedition = \"2021\"\n{dependencies}"
                ),
            )
            .unwrap();
        }
        let read_dependencies = || {
            let output =
                crate::cargo::run_cargo(root, &["generate-lockfile", "--offline"]).unwrap();
            assert!(output.status.success(), "{}", output.stderr);
            let metadata = cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
            workspace_lock_dependencies(&metadata, "binary").unwrap()
        };
        let released = read_dependencies();
        for (leaf, should_update) in [("normal-leaf", true), ("dev-leaf", false)] {
            let manifest = root.join(leaf).join("Cargo.toml");
            let original = fs_err::read_to_string(&manifest).unwrap();
            fs_err::write(&manifest, original.replace("1.0.0", "1.0.1")).unwrap();
            let local = read_dependencies();
            assert_eq!(
                are_dependencies_updated(&local, &released),
                should_update,
                "updating {leaf}"
            );
            fs_err::write(manifest, original).unwrap();
        }
    }

    #[test]
    fn workspace_lock_comparison_ignores_git_dev_dependency_with_plus_in_branch() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let git_path = directory.path().join("git-dependency");
        fs_err::create_dir_all(git_path.join("src")).unwrap();
        let git_repo = git_cmd::Repo::init(&git_path);
        fs_err::create_dir_all(git_path.join("leaf/src")).unwrap();
        fs_err::write(git_path.join("src/lib.rs"), "").unwrap();
        fs_err::write(git_path.join("leaf/src/lib.rs"), "").unwrap();
        fs_err::write(
            git_path.join("Cargo.toml"),
            "[package]\nname = \"shared\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\
             [dependencies]\ndev-leaf = { path = \"leaf\" }\n",
        )
        .unwrap();
        let leaf_manifest = git_path.join("leaf/Cargo.toml");
        let leaf = "[package]\nname = \"dev-leaf\"\nversion = \"1.0.0\"\nedition = \"2021\"\n";
        fs_err::write(&leaf_manifest, leaf).unwrap();
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
                let package = root.join(name);
                fs_err::create_dir_all(package.join("src")).unwrap();
                fs_err::write(package.join("src/lib.rs"), "").unwrap();
                fs_err::write(
                    package.join("Cargo.toml"),
                    format!("[package]\nname = {name:?}\nversion = \"1.0.0\"\nedition = \"2021\"\n{dependencies}"),
                )
                .unwrap();
            }
        }
        let run_cargo = |args: &[&str]| {
            let output = crate::cargo::run_cargo(&local, args).unwrap();
            assert!(output.status.success(), "{}", output.stderr);
        };
        run_cargo(&["generate-lockfile"]);
        fs_err::copy(local.join("Cargo.lock"), released.join("Cargo.lock")).unwrap();
        fs_err::write(&leaf_manifest, leaf.replace("1.0.0", "1.0.1")).unwrap();
        git_repo
            .add_all_and_commit("update dev dependency")
            .unwrap();
        run_cargo(&["update"]);

        let local_metadata = cargo_utils::get_manifest_metadata(&local.join("Cargo.toml")).unwrap();
        let released_metadata =
            cargo_utils::get_manifest_metadata(&released.join("Cargo.toml")).unwrap();
        assert!(
            fs_err::read_to_string(local.join("Cargo.lock"))
                .unwrap()
                .contains("feature%2Bnext")
        );
        assert!(
            !are_workspace_lock_dependencies_updated(&local_metadata, &released_metadata, "binary")
                .unwrap()
        );
        // The same update must still count for the package declaring the dev dependency.
        assert!(
            are_workspace_lock_dependencies_updated(&local_metadata, &released_metadata, "library")
                .unwrap()
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
            let package = directory.path().join(name);
            fs_err::create_dir_all(package.join("src")).unwrap();
            fs_err::write(package.join("src/lib.rs"), "").unwrap();
            fs_err::write(
                package.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nversion = \"1.0.0\"\nedition = \"2021\"\n"),
            )
            .unwrap();
        }
        fs_err::write(
            directory.path().join("binary/Cargo.toml"),
            "[package]\nname = \"binary\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\
             [dependencies]\nlibrary = { path = \"../library\" }\n",
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
                format!(
                    "[package]\nname = \"library\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\
                     [dependencies]\nshared = {{ {normal} }}\n\
                     [dev-dependencies]\nshared-test = {{ package = \"shared\", {dev} }}\n"
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
            let released = workspace_lock_dependencies(&metadata, "binary").unwrap();
            for (leaf, should_update) in [("dev-leaf", false), ("normal-leaf", true)] {
                let changed = lockfile.replace(
                    &format!("name = {leaf:?}\nversion = \"1.0.0\""),
                    &format!("name = {leaf:?}\nversion = \"1.0.1\""),
                );
                fs_err::write(directory.path().join("Cargo.lock"), changed).unwrap();
                let local = workspace_lock_dependencies(&metadata, "binary").unwrap();
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
