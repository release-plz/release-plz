use cargo_metadata::camino::Utf8Path;
use git_cmd::Repo;
use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::cmd::release_plz_cmd;

#[test]
fn update_workspace_with_detached_head() {
    update_detached_workspace(None);
}

#[test]
fn update_workspace_with_detached_head_and_explicit_repo_url() {
    update_detached_workspace(Some("https://github.com/test/explicit"));
}

fn update_detached_workspace(repo_url: Option<&str>) {
    let (temp_dir, repo) = init_workspace(
        &[
            ("one", "version = \"0.1.0\"\n"),
            ("two", "version = \"0.1.0\"\n"),
        ],
        "",
        "[workspace]\nsemver_check = false\n",
    );
    if repo_url.is_none() {
        repo.git(&["remote", "add", "origin", "https://github.com/test/project"])
            .unwrap();
    }
    // Model a colocated jj repository without requiring jj to be installed in CI.
    repo.git(&["checkout", "--detach"]).unwrap();
    // Both packages must be updated even though comparing the first walks older commits.
    for name in ["one", "two"] {
        fs_err::write(
            repo.directory().join(name).join("src/lib.rs"),
            format!("// Fix {name}\n"),
        )
        .unwrap();
        repo.add_all_and_commit(&format!("fix: update {name}"))
            .unwrap();
    }
    let original_commit = repo.current_commit_hash().unwrap();

    run_workspace_update(&temp_dir, &repo, repo_url);

    for name in ["one", "two"] {
        let package_dir = repo.directory().join(name);
        let manifest = fs_err::read_to_string(package_dir.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("version = \"0.1.1\""), "{manifest}");
        let changelog = fs_err::read_to_string(package_dir.join("CHANGELOG.md")).unwrap();
        assert!(changelog.contains(&format!("update {name}")), "{changelog}");
        let url = repo_url.unwrap_or("https://github.com/test/project");
        assert!(
            changelog.contains(&format!("{url}/compare/{name}-v0.1.0...{name}-v0.1.1")),
            "{changelog}"
        );
    }
    assert_eq!(repo.current_commit_hash().unwrap(), original_commit);
    assert!(repo.is_head_detached().unwrap());
}

#[test]
fn release_commits_leaves_filtered_workspace_unchanged() {
    let (temp_dir, repo) = workspace_with_feat_release_commits_filter(&[
        ("one", "version.workspace = true\n"),
        ("two", "version.workspace = true\n"),
    ]);
    change_package(&repo, "one", "fix: update one");
    change_package(&repo, "two", "chore: update two");

    run_workspace_update(&temp_dir, &repo, None);

    // Everything was committed before the update, so a clean repository proves
    // that neither Cargo.toml nor Cargo.lock was touched.
    repo.is_clean().unwrap();
    assert_locked_versions(repo.directory(), &[("one", "1.0.0"), ("two", "1.0.0")]);
}

#[test]
fn release_commits_preserves_shared_workspace_version_calculation() {
    let (temp_dir, repo) = workspace_with_feat_release_commits_filter(&[
        ("one", "version.workspace = true\n"),
        ("two", "version.workspace = true\n"),
    ]);
    change_package(&repo, "one", "feat: update one");
    change_package(&repo, "two", "fix!: update two");

    run_workspace_update(&temp_dir, &repo, None);

    // Changing the shared version also changes the filtered sibling's version,
    // so its breaking change still determines the workspace version.
    assert_locked_versions(repo.directory(), &[("one", "2.0.0"), ("two", "2.0.0")]);
    let changelog = fs_err::read_to_string(repo.directory().join("one/CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [2.0.0]"), "{changelog}");
}

#[test]
fn release_commits_does_not_bump_workspace_for_independent_release() {
    let (temp_dir, repo) = workspace_with_feat_release_commits_filter(&[
        ("one", "version.workspace = true\n"),
        ("two", "version = \"1.0.0\"\n"),
    ]);
    change_package(&repo, "one", "fix: update one");
    change_package(&repo, "two", "feat: update two");

    run_workspace_update(&temp_dir, &repo, None);

    // `one` inherits the workspace version, so it staying at 1.0.0 proves the
    // workspace version was not bumped.
    assert_locked_versions(repo.directory(), &[("one", "1.0.0"), ("two", "1.1.0")]);
    assert!(!repo.directory().join("one/CHANGELOG.md").exists());
    assert!(repo.directory().join("two/CHANGELOG.md").exists());
}

#[test]
fn release_commits_keeps_workspace_bump_for_dependency_updates() {
    let (temp_dir, repo) = workspace_with_feat_release_commits_filter(&[
        (
            "one",
            "version.workspace = true\n[dependencies]\ntwo = { path = \"../two\", version = \"=1.0.0\" }\n",
        ),
        ("two", "version = \"1.0.0\"\n"),
    ]);
    change_package(&repo, "one", "fix: update one");
    change_package(&repo, "two", "feat: update two");

    run_workspace_update(&temp_dir, &repo, None);

    // Although its own commits are filtered out, `one` must be updated because
    // its dependency requirement changes when `two` is released.
    assert_locked_versions(repo.directory(), &[("one", "1.0.1"), ("two", "1.1.0")]);
    let changelog = fs_err::read_to_string(repo.directory().join("one/CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [1.0.1]"), "{changelog}");
}

/// Creates a workspace at `1.0.0` whose packages are already tagged as released
/// and whose config only treats `feat:` commits as release commits.
fn workspace_with_feat_release_commits_filter(packages: &[(&str, &str)]) -> (Utf8TempDir, Repo) {
    let (temp_dir, repo) = init_workspace(
        packages,
        "\n[workspace.package]\nversion = \"1.0.0\"\n",
        "[workspace]\nsemver_check = false\nrelease_commits = \"^feat:\"\n",
    );
    repo.git(&["remote", "add", "origin", "https://github.com/test/project"])
        .unwrap();
    for (name, _) in packages {
        repo.git(&["tag", &format!("{name}-v1.0.0")]).unwrap();
    }
    (temp_dir, repo)
}

/// Creates a `project` workspace with its git repository and a matching `registry`
/// workspace, which models the published versions.
///
/// `packages` maps each package name to the manifest lines appended after
/// `[package] name/edition`, and `workspace_package_toml` is appended to the root
/// `Cargo.toml`.
fn init_workspace(
    packages: &[(&str, &str)],
    workspace_package_toml: &str,
    release_plz_toml: &str,
) -> (Utf8TempDir, Repo) {
    let temp_dir = Utf8TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("project");
    let registry_dir = temp_dir.path().join("registry");
    let members: Vec<_> = packages.iter().map(|(name, _)| name).collect();
    for dir in [&project_dir, &registry_dir] {
        fs_err::create_dir(dir).unwrap();
        fs_err::write(
            dir.join("Cargo.toml"),
            format!(
                "[workspace]\nmembers = {members:?}\nresolver = \"3\"\n{workspace_package_toml}"
            ),
        )
        .unwrap();
        for (name, manifest) in packages {
            let package_dir = dir.join(name);
            fs_err::create_dir_all(package_dir.join("src")).unwrap();
            fs_err::write(
                package_dir.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nedition = \"2024\"\n{manifest}"),
            )
            .unwrap();
            fs_err::write(package_dir.join("src/lib.rs"), "// Initial release\n").unwrap();
        }
        assert_cmd::Command::new("cargo")
            .current_dir(dir)
            .args(["generate-lockfile", "--offline"])
            .assert()
            .success();
    }
    fs_err::write(project_dir.join("release-plz.toml"), release_plz_toml).unwrap();
    let repo = Repo::init(&project_dir);
    (temp_dir, repo)
}

fn change_package(repo: &Repo, name: &str, commit_message: &str) {
    fs_err::write(
        repo.directory().join(name).join("src/lib.rs"),
        format!("// Updated {name}\n"),
    )
    .unwrap();
    repo.add_all_and_commit(commit_message).unwrap();
}

fn run_workspace_update(temp_dir: &Utf8TempDir, repo: &Repo, repo_url: Option<&str>) {
    let mut cmd = release_plz_cmd(&temp_dir.path().join("target"));
    cmd.current_dir(repo.directory())
        .args(["update", "--registry-manifest-path"])
        .arg(temp_dir.path().join("registry/Cargo.toml"));
    if let Some(url) = repo_url {
        cmd.args(["--repo-url", url]);
    }
    cmd.assert().success();
}

/// Reads the resolved package versions with `--locked`, which makes `cargo metadata`
/// fail if `Cargo.lock` is stale (the symptom of #3086), so this also asserts that
/// the lockfile was updated.
fn assert_locked_versions(project_dir: &Utf8Path, expected_versions: &[(&str, &str)]) {
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(project_dir)
        .other_options(vec!["--locked".to_string(), "--offline".to_string()])
        .exec()
        .unwrap();
    for (name, version) in expected_versions {
        let package = metadata.packages.iter().find(|p| p.name == *name).unwrap();
        assert_eq!(package.version.to_string(), *version, "package: {name}");
    }
}
