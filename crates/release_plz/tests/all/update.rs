use cargo_metadata::camino::Utf8Path;
use git_cmd::Repo;
use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::{assert_locked_versions, cmd::release_plz_cmd, locked_metadata};

#[test]
fn update_changelog_includes_distinct_author_and_committer() {
    for (body, expected) in [
        (
            "{{ commit.author.name }} <{{ commit.author.email }}> committed by {{ commit.committer.name }} <{{ commit.committer.email }}>",
            "Author Person <author@example.com> committed by Committer Person <committer@example.com>",
        ),
        ("{{ commit.author.email }}", "author@example.com"),
    ] {
        let config = format!(
            r#"
[workspace]
semver_check = false
[changelog]
body = """
{{% for commit in commits %}}
{body}
{{% endfor %}}
"""
"#
        );
        let (temp_dir, repo) = released_workspace(&[("one", "version = \"1.0.0\"\n")], "", &config);
        fs_err::write(repo.directory().join("one/src/lib.rs"), "// Updated\n").unwrap();
        repo.git(&["add", "."]).unwrap();
        repo.git(&[
            "-c",
            "user.name=Committer Person",
            "-c",
            "user.email=committer@example.com",
            "commit",
            "--author=Author Person <author@example.com>",
            "-m",
            "fix: update package",
        ])
        .unwrap();

        run_workspace_update(&temp_dir, &repo, None);

        let changelog = fs_err::read_to_string(repo.directory().join("one/CHANGELOG.md")).unwrap();
        assert!(changelog.contains(expected), "{changelog}");
    }
}

#[test]
fn changelog_include_uses_only_the_requested_packages_original_commits() {
    let config = r#"
[workspace]
semver_check = false
[[package]]
name = "one"
changelog_include = ["two", "two"]
[[package]]
name = "two"
changelog_include = ["three"]
"#;
    let (temp_dir, repo) = released_workspace(
        &[
            ("one", "version = \"1.0.0\"\n"),
            ("two", "version = \"1.0.0\"\n"),
            ("three", "version = \"1.0.0\"\n"),
        ],
        "",
        config,
    );
    for name in ["one", "two", "three"] {
        change_package(&repo, name, &format!("fix: update {name}"));
    }

    run_workspace_update(&temp_dir, &repo, None);

    for (name, included) in [
        ("one", vec!["one", "two"]),
        ("two", vec!["two", "three"]),
        ("three", vec!["three"]),
    ] {
        let changelog =
            fs_err::read_to_string(repo.directory().join(name).join("CHANGELOG.md")).unwrap();
        for source in ["one", "two", "three"] {
            assert_eq!(
                changelog.matches(&format!("update {source}")).count(),
                usize::from(included.contains(&source)),
                "{name}: {changelog}",
            );
        }
    }
}

#[tokio::test]
async fn update_relocated_workspace_preserves_original_manifests() {
    let (temp_dir, repo) = released_workspace(
        &[
            ("support", "version = \"1.0.0\"\n"),
            (
                "consumer",
                "version = \"1.0.0\"\n[dependencies]\nsupport = { path = \"../support\", version = \"=1.0.0\" }\n",
            ),
        ],
        "",
        "[workspace]\nsemver_check = false\n",
    );
    change_package(&repo, "support", "feat: update support");
    let metadata = locked_metadata(repo.directory());
    let copied = release_plz_core::copy_to_temp_dir(repo.directory()).unwrap();
    let copied_project = copied.path().join("project");
    let request = release_plz_core::update_request::UpdateRequest::new(metadata)
        .unwrap()
        .set_local_manifest(copied_project.join("Cargo.toml"))
        .unwrap()
        .with_registry_manifest_path(&temp_dir.path().join("registry/Cargo.toml"))
        .unwrap()
        .with_default_package_config(
            release_plz_core::UpdateConfig::default().with_semver_check(false),
        );

    release_plz_core::update(&request).await.unwrap();

    // release-pr retains the original metadata while updating the copied checkout.
    assert_locked_versions(
        &copied_project,
        &[("support", "1.1.0"), ("consumer", "1.0.1")],
    );
    repo.is_clean().unwrap();
    assert_locked_versions(
        repo.directory(),
        &[("support", "1.0.0"), ("consumer", "1.0.0")],
    );
}

#[test]
fn update_refreshes_package_files_for_each_historical_snapshot() {
    let (temp_dir, repo) = init_workspace(
        &[("one", "version = \"1.0.0\"\n")],
        "",
        "[workspace]\nsemver_check = false\n",
    );
    let package = repo.directory().join("one");
    fs_err::write(package.join("src/old.rs"), "pub fn added() {}\n").unwrap();
    repo.add_all_and_commit("feat: add module").unwrap();
    fs_err::rename(package.join("src/old.rs"), package.join("src/new.rs")).unwrap();
    repo.add_all_and_commit("fix: rename module").unwrap();

    run_workspace_update(&temp_dir, &repo, None);

    assert_locked_versions(repo.directory(), &[("one", "1.1.0")]);
    let changelog = fs_err::read_to_string(package.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("add module"), "{changelog}");
    assert!(changelog.contains("rename module"), "{changelog}");
    // There is no release tag: the initial snapshot bounds the walk by package
    // equality. Reusing HEAD's file list would miss that boundary and include
    // the initial commit, whose message Repo::init sets to "add README".
    assert!(!changelog.contains("add README"), "{changelog}");
}

#[test]
fn update_detects_readme_changes_outside_the_package() {
    let (temp_dir, repo) = init_workspace(
        &[("one", "version = \"1.0.0\"\nreadme = \"../README.md\"\n")],
        "",
        "[workspace]\nsemver_check = false\n",
    );
    fs_err::copy(
        repo.directory().join("README.md"),
        temp_dir.path().join("registry/README.md"),
    )
    .unwrap();
    run_workspace_update(&temp_dir, &repo, None);
    repo.is_clean().unwrap();

    // Cargo lists the external README under a generated package-relative name.
    // Package contents can compare equal, so the separate README check matters.
    fs_err::write(
        repo.directory().join("README.md"),
        "# Updated documentation\n",
    )
    .unwrap();
    repo.add_all_and_commit("fix: improve documentation")
        .unwrap();

    run_workspace_update(&temp_dir, &repo, None);

    assert_locked_versions(repo.directory(), &[("one", "1.0.1")]);
    let changelog = fs_err::read_to_string(repo.directory().join("one/CHANGELOG.md")).unwrap();
    assert!(changelog.contains("improve documentation"), "{changelog}");
    assert!(!changelog.contains("add README"), "{changelog}");
}

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
fn dependency_updates_match_renamed_packages_among_unrelated_updates() {
    let (temp_dir, repo) = released_workspace(
        &[
            ("support", "version = \"1.0.0\"\n"),
            ("unrelated", "version = \"1.0.0\"\n"),
            (
                "consumer",
                "version = \"1.0.0\"\n[target.'cfg(unix)'.build-dependencies]\nshared.workspace = true\n",
            ),
        ],
        "[workspace.dependencies]\nshared = { package = \"support\", path = \"support\", version = \"=1.0.0\" }\n",
        "[workspace]\nsemver_check = false\n",
    );
    change_package(&repo, "support", "feat: update support");
    change_package(&repo, "unrelated", "fix: update unrelated");

    run_workspace_update(&temp_dir, &repo, None);

    assert_locked_versions(
        repo.directory(),
        &[
            ("support", "1.1.0"),
            ("unrelated", "1.0.1"),
            ("consumer", "1.0.1"),
        ],
    );
    let changelog = fs_err::read_to_string(repo.directory().join("consumer/CHANGELOG.md")).unwrap();
    assert!(
        changelog.contains("updated the following local packages: support"),
        "{changelog}"
    );
    assert!(!changelog.contains("unrelated"), "{changelog}");
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

#[test]
fn local_dependencies_update_strategy_preserves_compatible_floors() {
    check_local_dependencies_update_strategy(
        "if-needed",
        "0.6.7",
        "fix: update two",
        "0.6.8",
        false,
    );
}

#[test]
fn local_dependencies_update_strategy_never_rewrites_requirements() {
    check_local_dependencies_update_strategy("never", "0.6.7", "fix: update two", "0.6.8", false);
}

#[test]
fn local_dependencies_update_strategy_keeps_default_behavior() {
    check_local_dependencies_update_strategy("", "0.6.7", "fix: update two", "0.6.8", true);
}

#[test]
fn local_dependencies_update_strategy_propagates_incompatible_releases() {
    check_local_dependencies_update_strategy(
        "if-needed",
        "0.6.7",
        "fix!: update two",
        "0.7.0",
        true,
    );
}

#[test]
fn local_dependencies_update_strategy_propagates_exact_requirements() {
    check_local_dependencies_update_strategy(
        "if-needed",
        "=0.6.7",
        "fix: update two",
        "0.6.8",
        true,
    );
}

#[test]
fn local_dependencies_update_strategy_retains_deliberate_minimum_increases() {
    let (temp_dir, repo) = init_workspace(
        &[
            (
                "one",
                "version = \"1.0.0\"\n[dependencies]\ntwo = { path = \"../two\", version = \"0.6.6\" }\n",
            ),
            ("two", "version = \"0.6.7\"\n"),
        ],
        "",
        "[workspace]\nsemver_check = false\nlocal_dependencies_update_strategy = \"if-needed\"\n",
    );
    for tag in ["one-v1.0.0", "two-v0.6.7"] {
        repo.git(&["tag", tag]).unwrap();
    }
    let path = repo.directory().join("one/Cargo.toml");
    let manifest = fs_err::read_to_string(&path)
        .unwrap()
        .replace("0.6.6", "0.6.7");
    fs_err::write(&path, manifest).unwrap();
    repo.add_all_and_commit("fix: require the dependency fix")
        .unwrap();

    run_workspace_update(&temp_dir, &repo, Some("https://github.com/test/project"));

    assert_locked_versions(repo.directory(), &[("one", "1.0.1"), ("two", "0.6.7")]);
    assert!(
        fs_err::read_to_string(path)
            .unwrap()
            .contains("version = \"0.6.7\"")
    );
    assert!(repo.directory().join("one/CHANGELOG.md").exists());
    assert!(!repo.directory().join("two/CHANGELOG.md").exists());
}

fn check_local_dependencies_update_strategy(
    policy: &str,
    requirement: &str,
    commit: &str,
    next_version: &str,
    dependent_updated: bool,
) {
    // Exercise direct requirements, renamed inherited requirements, and dependencies
    // on packages whose own version is inherited from workspace.package.
    for (inherited_dependency, inherited_version) in [(false, false), (true, false), (true, true)] {
        let dependencies = if inherited_dependency {
            "[dependencies]\nshared.workspace = true\n".to_string()
        } else {
            format!(
                "[dependencies]\nshared = {{ package = \"two\", path = \"../two\", version = {requirement:?} }}\n"
            )
        };
        let workspace_dependency = if inherited_dependency {
            format!(
                "[workspace.dependencies]\nshared = {{ package = \"two\", path = \"two\", version = {requirement:?} }}\n"
            )
        } else {
            String::new()
        };
        let two_version = if inherited_version {
            "version.workspace = true\n"
        } else {
            "version = \"0.6.7\"\n"
        };
        let policy_config = if policy.is_empty() {
            String::new()
        } else {
            format!("local_dependencies_update_strategy = {policy:?}\n")
        };
        let (temp_dir, repo) = init_workspace(
            &[
                ("one", &format!("version = \"1.0.0\"\n{dependencies}")),
                ("two", two_version),
                (
                    "three",
                    "version = \"1.0.0\"\n[dependencies]\none = { path = \"../one\", version = \"=1.0.0\" }\n",
                ),
            ],
            &format!("[workspace.package]\nversion = \"0.6.7\"\n{workspace_dependency}"),
            &format!("[workspace]\nsemver_check = false\n{policy_config}"),
        );
        for tag in ["one-v1.0.0", "two-v0.6.7", "three-v1.0.0"] {
            repo.git(&["tag", tag]).unwrap();
        }
        let requirement_path = if inherited_dependency {
            repo.directory().join("Cargo.toml")
        } else {
            repo.directory().join("one/Cargo.toml")
        };
        let original = fs_err::read_to_string(&requirement_path).unwrap();
        change_package(&repo, "two", commit);

        run_workspace_update(&temp_dir, &repo, Some("https://github.com/test/project"));

        let dependent_version = if dependent_updated { "1.0.1" } else { "1.0.0" };
        assert_locked_versions(
            repo.directory(),
            &[
                ("one", dependent_version),
                ("two", next_version),
                ("three", dependent_version),
            ],
        );
        for name in ["one", "three"] {
            assert_eq!(
                repo.directory().join(name).join("CHANGELOG.md").exists(),
                dependent_updated
            );
        }
        let manifest = fs_err::read_to_string(&requirement_path).unwrap();
        let new_requirement = if dependent_updated {
            format!(
                "{}{next_version}",
                if requirement.starts_with('=') {
                    "="
                } else {
                    ""
                }
            )
        } else {
            requirement.to_string()
        };
        assert!(
            manifest.contains(&format!("version = {new_requirement:?}")),
            "{manifest}"
        );
        if !dependent_updated && !inherited_version {
            assert_eq!(manifest, original);
        }
    }
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
