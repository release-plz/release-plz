use cargo_metadata::camino::Utf8Path;
use git_cmd::Repo;
use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::{assert_locked_versions, cmd::release_plz_cmd, locked_metadata};

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

#[test]
fn dependency_updates_preserve_shared_changelog_entries_and_previous_versions() {
    // Both packages are released as 1.0.1, the version of the entry that support
    // has just added to the shared changelog.
    update_shared_changelog(
        "fix: update support",
        "",
        &[
            "/compare/support-v1.0.0...support-v1.0.1",
            "/compare/consumer-v1.0.0...consumer-v1.0.1",
        ],
    );
}

#[test]
fn dependency_updates_preserve_shared_changelog_previous_versions_when_versions_differ() {
    update_shared_changelog(
        "feat: update support",
        "",
        &[
            "/compare/support-v1.0.0...support-v1.1.0",
            "/compare/consumer-v1.0.0...consumer-v1.0.1",
        ],
    );
}

#[test]
fn dependency_updates_pass_previous_versions_to_shared_changelog_template() {
    update_shared_changelog(
        "fix: update support",
        PREVIOUS_VERSION_CHANGELOG_CONFIG,
        &[
            "## [1.0.1] support previous=1.0.0",
            "## [1.0.1] consumer previous=1.0.0",
        ],
    );
}

#[test]
fn dependency_updates_pass_previous_versions_to_shared_changelog_template_when_versions_differ() {
    update_shared_changelog(
        "feat: update support",
        PREVIOUS_VERSION_CHANGELOG_CONFIG,
        &[
            "## [1.1.0] support previous=1.0.0",
            "## [1.0.1] consumer previous=1.0.0",
        ],
    );
}

#[test]
fn shared_changelog_previous_version_of_bumped_package_is_its_registry_version() {
    let (temp_dir, repo) = released_workspace(
        &[
            ("published", "version.workspace = true\n"),
            ("unpublished", "version.workspace = true\n"),
        ],
        "\n[workspace.package]\nversion = \"1.0.0\"\n",
        &format!(
            "[workspace]\nsemver_check = false\nchangelog_path = \"CHANGELOG.md\"\n{PREVIOUS_VERSION_CHANGELOG_CONFIG}"
        ),
    );
    // Bump the workspace version, but only publish `published` at 1.1.0.
    let path = repo.directory().join("Cargo.toml");
    let manifest = fs_err::read_to_string(&path).unwrap();
    fs_err::write(path, manifest.replace("1.0.0", "1.1.0")).unwrap();
    generate_lockfile(repo.directory());
    repo.add_all_and_commit("chore: bump workspace version")
        .unwrap();
    repo.tag_lightweight("published-v1.1.0").unwrap();
    let registry_dir = temp_dir.path().join("registry");
    let path = registry_dir.join("published/Cargo.toml");
    let manifest = fs_err::read_to_string(&path).unwrap();
    fs_err::write(
        path,
        manifest.replace("version.workspace = true", "version = \"1.1.0\""),
    )
    .unwrap();
    generate_lockfile(&registry_dir);
    change_package(&repo, "published", "fix: update published");
    change_package(&repo, "unpublished", "fix: update unpublished");

    let summary = run_workspace_update(&temp_dir, &repo, None);

    assert!(
        summary.contains("`unpublished`: 1.0.0 -> 1.1.1"),
        "{summary}"
    );
    let changelog = fs_err::read_to_string(repo.directory().join("CHANGELOG.md")).unwrap();
    // `unpublished` never released 1.1.0, its local version.
    for line in [
        "## [1.1.1] published previous=1.1.0",
        "## [1.1.1] unpublished previous=1.0.0",
    ] {
        assert!(changelog.contains(line), "{line}: {changelog}");
    }
}

/// Renders the package name and its previous version in each release header.
const PREVIOUS_VERSION_CHANGELOG_CONFIG: &str = r#"
[changelog]
body = """
## [{{ version }}] {{ package }} previous={{ previous.version | default(value="none") }}
{% for commit in commits %}- {{ commit.message }}
{% endfor %}
"""
"#;

/// Releases `support` with `support_commit` and its dependent `consumer`, which
/// share a changelog, and checks that the changelog keeps the entries of both
/// packages and contains every `expected` line.
fn update_shared_changelog(support_commit: &str, changelog_config: &str, expected: &[&str]) {
    let (temp_dir, repo) = released_workspace(
        &[
            ("support", "version = \"1.0.0\"\n"),
            (
                "consumer",
                "version = \"1.0.0\"\n[dependencies]\nsupport = { path = \"../support\", version = \"=1.0.0\" }\n",
            ),
        ],
        "",
        &format!(
            "[workspace]\nsemver_check = false\nchangelog_path = \"CHANGELOG.md\"\n{changelog_config}"
        ),
    );
    change_package(&repo, "support", support_commit);

    run_workspace_update(&temp_dir, &repo, None);

    let changelog = fs_err::read_to_string(repo.directory().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("update support"), "{changelog}");
    assert!(
        changelog.contains("updated the following local packages: support"),
        "{changelog}"
    );
    for line in expected {
        assert!(changelog.contains(line), "{line}: {changelog}");
    }
}

#[test]
fn shared_changelog_keeps_entry_matching_another_package_release() {
    let (temp_dir, repo) = released_workspace(
        &[
            ("support", "version = \"1.0.1\"\n"),
            (
                "consumer",
                "version = \"1.0.0\"\n[dependencies]\nsupport = { path = \"../support\", version = \"1.0.1\" }\n",
            ),
        ],
        "",
        "[workspace]\nsemver_check = false\nchangelog_path = \"CHANGELOG.md\"\n",
    );
    // The latest entry of the shared changelog belongs to `support`, but it has
    // the version that `consumer` is about to release.
    fs_err::write(
        repo.directory().join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n## [1.0.1] - 2026-01-01\n\n- release support\n",
    )
    .unwrap();
    repo.add_all_and_commit("chore: release").unwrap();
    change_package(&repo, "support", "fix: update support");
    change_package(&repo, "consumer", "fix: update consumer");

    let summary = run_workspace_update(&temp_dir, &repo, None);

    assert!(summary.contains("`consumer`: 1.0.0 -> 1.0.1"), "{summary}");
    let changelog = fs_err::read_to_string(repo.directory().join("CHANGELOG.md")).unwrap();
    for entry in ["update support", "update consumer", "release support"] {
        assert!(changelog.contains(entry), "{changelog}");
    }
}

#[test]
fn dependency_updates_propagate_through_the_shared_workspace_version() {
    update_through_shared_workspace_version("", None, "1.0.1", true);
}

#[test]
fn dependency_updates_propagate_through_a_release_commits_filtered_sibling() {
    update_through_shared_workspace_version("release_commits = \"^feat:\"\n", None, "1.0.1", false);
}

#[test]
fn dependency_updates_propagate_through_a_breaking_filtered_sibling() {
    // A filtered breaking change still determines the shared version once
    // the consumer needs a dependency-only release.
    let (_temp_dir, repo) = update_through_shared_workspace_version(
        "release_commits = \"^feat:\"\n",
        Some("fix!: update sibling"),
        "2.0.0",
        false,
    );
    let changelog = fs_err::read_to_string(repo.directory().join("consumer/CHANGELOG.md")).unwrap();
    assert!(!changelog.contains("## [1.0.1]"), "{changelog}");
}

#[test]
fn dependency_updates_propagate_through_a_release_disabled_sibling() {
    update_through_shared_workspace_version(
        "[[package]]\nname = \"sibling\"\nrelease = false\n",
        None,
        "1.0.1",
        false,
    );
}

/// Updates a workspace after a `feat:` commit to `support` and, if given,
/// `sibling_commit` to `sibling`. `consumer` depends on `support` and shares the
/// workspace version with `sibling`, which `downstream` depends on, so the
/// release of `support` must reach `downstream` even when `release_sibling` is
/// false.
fn update_through_shared_workspace_version(
    release_plz_toml: &str,
    sibling_commit: Option<&str>,
    shared_version: &str,
    release_sibling: bool,
) -> (Utf8TempDir, Repo) {
    let (temp_dir, repo) = released_workspace(
        &[
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
        ],
        "\n[workspace.package]\nversion = \"1.0.0\"\n",
        &format!("[workspace]\nsemver_check = false\n{release_plz_toml}"),
    );
    change_package(&repo, "support", "feat: update support");
    if let Some(commit_message) = sibling_commit {
        change_package(&repo, "sibling", commit_message);
    }

    let summary = run_workspace_update(&temp_dir, &repo, None);

    assert_locked_versions(
        repo.directory(),
        &[
            ("support", "1.1.0"),
            ("consumer", shared_version),
            ("sibling", shared_version),
            ("downstream", "1.0.1"),
        ],
    );
    let mut released = vec![("consumer", shared_version), ("downstream", "1.0.1")];
    if release_sibling {
        released.push(("sibling", shared_version));
    } else {
        // Skipping the sibling's release doesn't stop its inherited version from
        // changing, which must still propagate to downstream.
        assert!(!repo.directory().join("sibling/CHANGELOG.md").exists());
        assert!(!summary.contains("`sibling`"), "{summary}");
    }
    for (name, version) in released {
        let changelog =
            fs_err::read_to_string(repo.directory().join(name).join("CHANGELOG.md")).unwrap();
        assert!(
            changelog.contains(&format!("## [{version}]")),
            "{name}: {changelog}"
        );
        assert!(
            summary.contains(&format!("`{name}`: 1.0.0 -> {version}")),
            "{summary}"
        );
    }
    (temp_dir, repo)
}

#[test]
fn dependency_updates_respect_prerelease_workspace_versions() {
    update_workspace_version_consumer("", "1.0.0-alpha.1", None, "1.0.0-alpha.2");
}

#[test]
fn dependency_updates_respect_pre_bumped_workspace_versions() {
    update_workspace_version_consumer("", "1.0.0", Some("1.1.0"), "1.1.0");
}

#[test]
fn dependency_updates_respect_pre_bumped_workspace_versions_with_release_commits_filter() {
    update_workspace_version_consumer(
        "release_commits = \"^feat:\"\n",
        "1.0.0",
        Some("1.1.0"),
        "1.1.0",
    );
}

/// Releases `support` in a workspace published at `published_version`, whose
/// `consumer` depends on `support` and inherits the workspace version, which is
/// manually set to `bumped_version` after the release, if any.
fn update_workspace_version_consumer(
    release_plz_toml: &str,
    published_version: &str,
    bumped_version: Option<&str>,
    next_version: &str,
) {
    let (temp_dir, repo) = released_workspace(
        &[
            ("support", "version = \"1.0.0\"\n"),
            (
                "consumer",
                "version.workspace = true\n[dependencies]\nsupport = { path = \"../support\", version = \"=1.0.0\" }\n",
            ),
        ],
        &format!("\n[workspace.package]\nversion = \"{published_version}\"\n"),
        &format!("[workspace]\nsemver_check = false\n{release_plz_toml}"),
    );
    if let Some(bumped_version) = bumped_version {
        let path = repo.directory().join("Cargo.toml");
        let manifest = fs_err::read_to_string(&path).unwrap();
        fs_err::write(path, manifest.replace(published_version, bumped_version)).unwrap();
        generate_lockfile(repo.directory());
        repo.add_all_and_commit("chore: bump workspace version")
            .unwrap();
    }
    change_package(&repo, "support", "feat: update support");

    let summary = run_workspace_update(&temp_dir, &repo, None);

    assert_locked_versions(
        repo.directory(),
        &[("consumer", next_version), ("support", "1.1.0")],
    );
    assert!(
        summary.contains(&format!(
            "`consumer`: {published_version} -> {next_version}"
        )),
        "{summary}"
    );
    let changelog = fs_err::read_to_string(repo.directory().join("consumer/CHANGELOG.md")).unwrap();
    assert!(
        changelog.contains(&format!("## [{next_version}]")),
        "{changelog}"
    );
}

/// Creates a workspace at `1.0.0` whose packages are already tagged as released
/// and whose config only treats `feat:` commits as release commits.
fn workspace_with_feat_release_commits_filter(packages: &[(&str, &str)]) -> (Utf8TempDir, Repo) {
    released_workspace(
        packages,
        "\n[workspace.package]\nversion = \"1.0.0\"\n",
        "[workspace]\nsemver_check = false\nrelease_commits = \"^feat:\"\n",
    )
}

/// Creates a workspace with [`init_workspace`] whose packages are already tagged
/// as released at their current version, and whose `origin` remote is on GitHub.
fn released_workspace(
    packages: &[(&str, &str)],
    workspace_package_toml: &str,
    release_plz_toml: &str,
) -> (Utf8TempDir, Repo) {
    let (temp_dir, repo) = init_workspace(packages, workspace_package_toml, release_plz_toml);
    repo.git(&["remote", "add", "origin", "https://github.com/test/project"])
        .unwrap();
    for package in locked_metadata(repo.directory()).workspace_packages() {
        repo.tag_lightweight(&format!("{}-v{}", package.name, package.version))
            .unwrap();
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
        generate_lockfile(dir);
    }
    fs_err::write(project_dir.join("release-plz.toml"), release_plz_toml).unwrap();
    let repo = Repo::init(&project_dir);
    (temp_dir, repo)
}

fn generate_lockfile(dir: &Utf8Path) {
    assert_cmd::Command::new("cargo")
        .current_dir(dir)
        .args(["generate-lockfile", "--offline"])
        .assert()
        .success();
}

fn change_package(repo: &Repo, name: &str, commit_message: &str) {
    fs_err::write(
        repo.directory().join(name).join("src/lib.rs"),
        format!("// Updated {name}\n"),
    )
    .unwrap();
    repo.add_all_and_commit(commit_message).unwrap();
}

fn run_workspace_update(temp_dir: &Utf8TempDir, repo: &Repo, repo_url: Option<&str>) -> String {
    let mut cmd = release_plz_cmd(&temp_dir.path().join("target"));
    cmd.current_dir(repo.directory())
        .args(["update", "--registry-manifest-path"])
        .arg(temp_dir.path().join("registry/Cargo.toml"));
    if let Some(url) = repo_url {
        cmd.args(["--repo-url", url]);
    }
    let output = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(output).unwrap()
}
