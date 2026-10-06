use cargo_metadata::{camino::Utf8Path, semver::Version};
use git_cmd::Repo;

use super::{UpdateConfig, UpdateRequest, update};
use crate::{
    copy_to_temp_dir,
    fs_utils::{self, Utf8TempDir},
    test_utils::{generate_lockfile, init_baseline_repo, write_package_with_manifest},
};

/// Creates a `project` workspace at `1.0.0` whose packages are already tagged as
/// released. `packages` maps each package name to the manifest lines appended
/// after `[package] name/edition`.
fn workspace(packages: &[(&str, &str)]) -> (Utf8TempDir, Repo) {
    let dir = Utf8TempDir::new().unwrap();
    // Resolve symlinks (such as macOS's /var) so metadata and project paths agree.
    let root = fs_utils::canonicalize_utf8(dir.path()).unwrap();
    let repo = init_baseline_repo(&root.join("project"), |root| {
        let members: Vec<_> = packages.iter().map(|(name, _)| name).collect();
        fs_err::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nmembers = {members:?}\nresolver = \"3\"\n[workspace.package]\nversion = \"1.0.0\"\n"
            ),
        )
        .unwrap();
        for (name, manifest) in packages {
            write_package_with_manifest(
                &root.join(name),
                &format!("[package]\nname = {name:?}\nedition = \"2024\"\n{manifest}"),
            );
        }
    });
    for (name, _) in packages {
        repo.tag_lightweight(&format!("{name}-v1.0.0")).unwrap();
    }
    (dir, repo)
}

fn relocated_request(repo: &Repo) -> (Utf8TempDir, UpdateRequest) {
    let metadata =
        cargo_utils::get_manifest_metadata(&repo.directory().join("Cargo.toml")).unwrap();
    let relocated = copy_to_temp_dir(repo.directory()).unwrap();
    // release-pr retains metadata from the original checkout when it moves the
    // request to the temporary repository where the release will be prepared.
    let request = UpdateRequest::new(metadata)
        .unwrap()
        .with_default_package_config(UpdateConfig {
            git_only: Some(true),
            semver_check: false,
            ..UpdateConfig::default()
        })
        .set_local_manifest(relocated.path().join("project/Cargo.toml"))
        .unwrap();
    (relocated, request)
}

fn assert_locked_versions(root: &Utf8Path, expected: &[(&str, &str)]) {
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(root)
        .other_options(vec!["--locked".to_string(), "--offline".to_string()])
        .exec()
        .unwrap();
    for (name, version) in expected {
        let package = metadata.packages.iter().find(|p| p.name == *name).unwrap();
        assert_eq!(package.version.to_string(), *version, "{name}");
    }
}

#[tokio::test]
async fn relocated_shared_versions_propagate_through_unreleased_siblings() {
    let (_dir, repo) = workspace(&[
        ("support", "version = \"1.0.0\"\n"),
        (
            "consumer",
            "version.workspace = true\n[dependencies]\nsupport = { path = \"../support\", version = \"=1.0.0\" }\n",
        ),
        ("sibling", "version.workspace = true\n"),
        (
            "downstream",
            "version = \"1.0.0\"\n[dependencies]\nsibling = { path = \"../sibling\", version = \"=1.0.0\" }\n",
        ),
    ]);
    fs_err::write(
        repo.directory().join("support/src/lib.rs"),
        "pub fn new() {}\n",
    )
    .unwrap();
    repo.add_all_and_commit("feat: update support").unwrap();
    let (_relocated, request) = relocated_request(&repo);
    // Disabling its release leaves the sibling out of the relocated project, so
    // the update only knows it from the metadata of the original checkout.
    let mut config = request.get_package_config("sibling");
    config.generic.release = false;
    let request = request.with_package_config("sibling", config);

    let (updates, _repository) = update(&request).await.unwrap();

    let downstream = updates
        .updates()
        .iter()
        .find(|(p, _)| p.name == "downstream")
        .expect("the inherited sibling's new version must release downstream");
    assert_eq!(downstream.1.version, Version::new(1, 0, 1));
    let root = request.local_manifest_dir().unwrap();
    let changelog = fs_err::read_to_string(root.join("downstream/CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [1.0.1]"), "{changelog}");
    assert!(changelog.contains("sibling"), "{changelog}");
    assert!(!root.join("sibling/CHANGELOG.md").exists());
    assert_locked_versions(
        root,
        &[
            ("support", "1.1.0"),
            ("consumer", "1.0.1"),
            ("sibling", "1.0.1"),
            ("downstream", "1.0.1"),
        ],
    );
    repo.is_clean().unwrap();
}

#[tokio::test]
async fn relocated_pre_bumped_shared_versions_update_dependency_requirements() {
    let (_dir, repo) = workspace(&[
        ("upstream", "version.workspace = true\n"),
        (
            "downstream",
            "version = \"1.0.0\"\n[dependencies]\nupstream = { path = \"../upstream\", version = \"^1.0.0\" }\n",
        ),
    ]);
    let path = repo.directory().join("Cargo.toml");
    let manifest = fs_err::read_to_string(&path).unwrap();
    fs_err::write(path, manifest.replace("1.0.0", "1.1.0")).unwrap();
    fs_err::write(
        repo.directory().join("upstream/src/lib.rs"),
        "pub fn new() {}\n",
    )
    .unwrap();
    generate_lockfile(repo.directory());
    repo.add_all_and_commit("feat: update upstream and bump workspace version")
        .unwrap();
    let (_relocated, request) = relocated_request(&repo);

    let (updates, _repository) = update(&request).await.unwrap();

    let upstream = updates
        .updates()
        .iter()
        .find(|(p, _)| p.name == "upstream")
        .unwrap();
    assert_eq!(upstream.0.version, Version::new(1, 1, 0));
    assert_eq!(upstream.1.version, Version::new(1, 1, 0));
    let root = request.local_manifest_dir().unwrap();
    let downstream = fs_err::read_to_string(root.join("downstream/Cargo.toml")).unwrap();
    // The old caret requirement still resolves, but the release must update its
    // lower bound to the upstream version it was prepared against.
    assert!(downstream.contains("version = \"^1.1.0\""), "{downstream}");
    assert_locked_versions(root, &[("upstream", "1.1.0"), ("downstream", "1.0.1")]);
    repo.is_clean().unwrap();
}
