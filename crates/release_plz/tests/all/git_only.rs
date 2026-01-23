use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::{test_context::TestContext, today};

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_default_tag_name() {
    let context = TestContext::new().await;

    // Configure git_only (tag name defaults to "v{version}")
    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    context.run_release_pr().success();

    // Verify PR was created with correct version bump (0.1.0 -> 0.1.1)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_custom_tag_name() {
    let context = TestContext::new().await;

    // Configure with a custom tag name template
    let config = r#"
[workspace]
git_only = true
git_tag_name = "release-{{ version }}-prod"
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context
        .repo
        .tag("release-0.1.0-prod", "Release 0.1.0 production")
        .unwrap();

    // Make a feature commit
    let new_file = context.repo_dir().join("src").join("new.rs");
    fs_err::write(&new_file, "// New feature").unwrap();
    context.push_all_changes("feat: add new module");

    // Run release-pr
    context.run_release_pr().success();

    // Verify PR was created
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    // The PR title uses the standard format, not the tag format
    // feat commits do patch bump in 0.x without features_always_increment_minor
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_finds_highest_version_tag() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create tags with corresponding Cargo.toml versions
    // In git_only mode, tags should point to commits where Cargo.toml version matches the tag
    use cargo_metadata::semver::Version;

    // Tag v0.1.0 (Cargo.toml already has 0.1.0 from cargo init)
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Update to 0.1.5, commit, and tag
    context.set_package_version(&context.gitea.repo, &Version::parse("0.1.5").unwrap());
    context.push_all_changes("chore: bump version to 0.1.5");
    context.repo.tag("v0.1.5", "Release v0.1.5").unwrap();

    // Update to 0.2.0, commit, and tag
    context.set_package_version(&context.gitea.repo, &Version::parse("0.2.0").unwrap());
    context.push_all_changes("chore: bump version to 0.2.0");
    context.repo.tag("v0.2.0", "Release v0.2.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    context.run_release_pr().success();

    // Verify it uses v0.2.0 as base (highest), bumps to v0.2.1
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.2.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_ignores_non_matching_tags() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create tags with different patterns
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();
    context.repo.tag("release-0.2.0", "Release 0.2.0").unwrap();
    context.repo.tag("beta-0.3.0", "Beta 0.3.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    context.run_release_pr().success();

    // Verify it only considers v0.1.0 (matching pattern)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_no_matching_tag_creates_initial_release() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create tag that doesn't match the default "v" pattern
    context.repo.tag("release-0.1.0", "Release 0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    let _outcome = context.run_release_pr().success();

    // Verify PR was created for initial release
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(
        opened_prs.len(),
        1,
        "Expected PR for initial release when no matching tag exists"
    );

    // Verify it's a release PR with a version
    let pr = &opened_prs[0];
    assert_eq!(pr.title, "chore: release v0.1.0");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_no_tags_creates_initial_release() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Don't create any tags

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    let _outcome = context.run_release_pr().success();

    // Verify PR was created for initial release
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(
        opened_prs.len(),
        1,
        "Expected PR for initial release when no tags exist"
    );

    // Verify it's a release PR with a version
    let pr = &opened_prs[0];
    assert_eq!(pr.title, "chore: release v0.1.0");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_feat_commit_minor_bump() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
features_always_increment_minor = true
"#;
    context.write_release_plz_toml(config);

    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    let readme = context.repo_dir().join("README.md");

    // Make a fix commit
    fs_err::write(&readme, "# Fix 1").unwrap();
    context.push_all_changes("fix: first fix");

    // Make a feature commit
    let new_file = context.repo_dir().join("src").join("feature.rs");
    fs_err::write(&new_file, "// New feature").unwrap();
    context.push_all_changes("feat: add new feature");

    context.run_release_pr().success();

    // Verify minor bump (0.1.0 -> 0.2.0)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.2.0");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_respects_release_commits_regex() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
release_commits = "^feat:"
"#;
    context.write_release_plz_toml(config);

    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a fix commit (doesn't match regex)
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Fixed README").unwrap();
    context.push_all_changes("fix: correct readme");

    // Run release-pr - should not create PR
    let outcome = context.run_release_pr().success();
    outcome.stdout("{\"prs\":[]}\n");
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 0);

    // Now make a feature commit (matches regex)
    let new_file = context.repo_dir().join("src").join("feature.rs");
    fs_err::write(&new_file, "// New feature").unwrap();
    context.push_all_changes("feat: add new feature");

    // Run release-pr - should create PR
    context.run_release_pr().success();
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
}

// ============================================================================
// Workspace vs Package-Level Config
// ============================================================================

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_workspace_level_applies_to_all() {
    let context = TestContext::new_workspace(&["lib1", "lib2"]).await;

    // In workspace git_only mode, each package needs a unique tag pattern to distinguish tags
    // Using {{ package }} variable which will be replaced with the package name
    let config = r#"
[workspace]
git_only = true
git_tag_name = "{{ package }}-v{{ version }}"
"#;
    context.write_release_plz_toml(config);

    // Create tags for each package with their unique prefixes
    context
        .repo
        .tag("lib1-v0.1.0", "Release lib1 v0.1.0")
        .unwrap();
    context
        .repo
        .tag("lib2-v0.1.0", "Release lib2 v0.1.0")
        .unwrap();

    // Make changes to one package (workspace members are bins by default, so modify main.rs)
    let lib1_file = context.package_path("lib1").join("src").join("main.rs");
    fs_err::write(&lib1_file, "fn main() { println!(\"updated\"); }").unwrap();
    context.push_all_changes("feat: update lib1");

    context.run_release_pr().success();

    // Only lib1 should be in the release (it's the only one that changed)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    let pr_body = opened_prs[0].body.as_ref().unwrap();

    let today = today();
    let username = context.gitea.user.username();
    let repo = &context.gitea.repo;
    assert_eq!(
        format!(
            r"
## 🤖 New release

* `lib1`: 0.1.0 -> 0.1.1

<details><summary><i><b>Changelog</b></i></summary><p>

<blockquote>

## [0.1.1](https://localhost/{username}/{repo}/compare/lib1-v0.1.0...lib1-v0.1.1) - {today}

### Added

- update lib1
</blockquote>


</p></details>

---
This PR was generated with [release-plz](https://github.com/release-plz/release-plz/)."
        )
        .trim(),
        pr_body.trim()
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_per_package_prefix() {
    let context = TestContext::new_workspace(&["api", "core"]).await;

    // Workspace level: git_only with default template "{{ package }}-v{{ version }}"
    // Package "api": override with custom template "api-v{{ version }}"
    let config = r#"
[workspace]
git_only = true

[[package]]
name = "api"
git_tag_name = "api-v{{ version }}"
"#;
    context.write_release_plz_toml(config);

    // Tag the initial state (Cargo.toml already has 0.1.0)
    // In a workspace, each package needs its own tag with its configured template
    // api has custom template "api-v{{ version }}", core inherits workspace default "{{ package }}-v{{ version }}"
    context
        .repo
        .tag("api-v0.1.0", "Release api v0.1.0")
        .unwrap();
    context
        .repo
        .tag("core-v0.1.0", "Release core v0.1.0")
        .unwrap();

    // Make changes to api package
    let api_file = context.package_path("api").join("src").join("lib.rs");
    fs_err::write(&api_file, "pub fn api_updated() {}").unwrap();
    context.push_all_changes("feat: update api");

    context.run_release_pr().success();

    // Verify PR is created with correct content
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);

    let pr_body = opened_prs[0].body.as_ref().expect("PR should have body");

    let today = today();
    let username = context.gitea.user.username();
    let repo = &context.gitea.repo;
    assert_eq!(
        format!(
            r"
## 🤖 New release

* `api`: 0.1.0 -> 0.1.1 (✓ API compatible changes)

<details><summary><i><b>Changelog</b></i></summary><p>

<blockquote>

## [0.1.1](https://localhost/{username}/{repo}/compare/api-v0.1.0...api-v0.1.1) - {today}

### Added

- update api
</blockquote>


</p></details>

---
This PR was generated with [release-plz](https://github.com/release-plz/release-plz/)."
        )
        .trim(),
        pr_body.trim()
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_lightweight_tags() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context.repo.tag_lightweight("v0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    context.run_release_pr().success();

    // Verify PR was created with correct version bump (0.1.0 -> 0.1.1)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_mixed_annotated_and_lightweight_tags() {
    use cargo_metadata::semver::Version;

    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create mix of tags with proper version updates
    // Tag v0.1.0 (Cargo.toml already has 0.1.0 from cargo init)
    context.repo.tag_lightweight("v0.1.0").unwrap();

    // Update to 0.1.5, commit, and tag as annotated
    context.set_package_version(&context.gitea.repo, &Version::parse("0.1.5").unwrap());
    context.push_all_changes("chore: bump version to 0.1.5");
    context.repo.tag("v0.1.5", "Release v0.1.5").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    context.run_release_pr().success();

    // Should use v0.1.5 (highest version) and bump to v0.1.6
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.6");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_invalid_tag_format() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create tags with invalid semver formats
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();
    context.repo.tag("v1.2.invalid", "Invalid version").unwrap();
    context.repo.tag("vNOT_A_VERSION", "Not a version").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr - should succeed using v0.1.0 and ignoring invalid tags
    context.run_release_pr().success();

    // Should use v0.1.0 (only valid tag) and bump to v0.1.1
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_breaking_change() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
features_always_increment_minor = true
"#;
    context.write_release_plz_toml(config);

    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a breaking change commit (using ! syntax)
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Breaking change").unwrap();
    context.push_all_changes("feat!: breaking API change");

    // Run release-pr
    context.run_release_pr().success();

    // Breaking change in 0.x should trigger minor bump (0.1.0 -> 0.2.0)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.2.0");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_multiple_packages_changed_workspace() {
    let context = TestContext::new_workspace(&["pkg1", "pkg2", "pkg3"]).await;

    // Use workspace-level template with {{ package }} variable for all packages
    let config = r#"
[workspace]
git_only = true
git_tag_name = "{{ package }}-v{{ version }}"
"#;
    context.write_release_plz_toml(config);

    // Create tags for all packages
    context
        .repo
        .tag("pkg1-v0.1.0", "Release pkg1 v0.1.0")
        .unwrap();
    context
        .repo
        .tag("pkg2-v0.1.0", "Release pkg2 v0.1.0")
        .unwrap();
    context
        .repo
        .tag("pkg3-v0.1.0", "Release pkg3 v0.1.0")
        .unwrap();

    // Make changes to multiple packages
    let pkg1_file = context.package_path("pkg1").join("src").join("main.rs");
    fs_err::write(&pkg1_file, "fn main() { println!(\"pkg1 updated\"); }").unwrap();

    let pkg2_file = context.package_path("pkg2").join("src").join("main.rs");
    fs_err::write(&pkg2_file, "fn main() { println!(\"pkg2 updated\"); }").unwrap();

    context.push_all_changes("feat: update pkg1 and pkg2");

    // Run release-pr
    context.run_release_pr().success();

    // Should create single PR with both changed packages
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);

    let today = today();
    let username = context.gitea.user.username();
    let repo = &context.gitea.repo;
    let pr_body = opened_prs[0].body.as_ref().unwrap();
    assert_eq!(
        format!(
            "
## 🤖 New release

* `pkg1`: 0.1.0 -> 0.1.1
* `pkg2`: 0.1.0 -> 0.1.1

<details><summary><i><b>Changelog</b></i></summary><p>

## `pkg1`

<blockquote>

## [0.1.1](https://localhost/{username}/{repo}/compare/pkg1-v0.1.0...pkg1-v0.1.1) - {today}

### Added

- update pkg1 and pkg2
</blockquote>

## `pkg2`

<blockquote>

## [0.1.1](https://localhost/{username}/{repo}/compare/pkg2-v0.1.0...pkg2-v0.1.1) - {today}

### Added

- update pkg1 and pkg2
</blockquote>


</p></details>

---
This PR was generated with [release-plz](https://github.com/release-plz/release-plz/)."
        )
        .trim(),
        pr_body.trim()
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_publish_enabled_fails_validation() {
    let context = TestContext::new().await;

    // Configure with both git_only = true and publish not disabled
    // This should fail validation because they are mutually exclusive
    let config = r#"
[workspace]
git_only = true
publish = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr should fail due to validation error
    let error = context.run_release_pr().failure().to_string();
    assert!(
        error.contains("git_only")
            && error.contains("publish")
            && error.contains("mutually exclusive"),
        "Expected validation error about git_only and publish being mutually exclusive, got: {error}"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_publish_disabled_succeeds() {
    let context = TestContext::new().await;

    // Configure with git_only = true and publish = false - this is valid
    let config = r#"
[workspace]
git_only = true
publish = false
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr should succeed
    context.run_release_pr().success();

    // Verify PR was created
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_workspace_with_package_publish_enabled_fails() {
    let context = TestContext::new_workspace(&["pkg-a", "pkg-b"]).await;

    // Workspace has git_only = true, but pkg-a has publish = true
    // This should fail validation
    let config = r#"
[workspace]
git_only = true

[[package]]
name = "pkg-a"
publish = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tags
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a commit to pkg-a
    let readme = context.package_path("pkg-a").join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update pkg-a readme");

    // Run release-pr should fail due to validation error
    let error = context.run_release_pr().failure().to_string();
    assert!(
        error.contains("git_only")
            && error.contains("publish")
            && error.contains("mutually exclusive"),
        "Expected validation error about git_only and publish being mutually exclusive, got: {error}",
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_publish_enabled_fails_validation_release_cmd() {
    let context = TestContext::new().await;

    // Configure with both git_only = true and publish = true
    // This should fail validation because they are mutually exclusive
    let config = r#"
[workspace]
git_only = true
publish = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release command should fail due to validation error
    let error = context.run_release().failure().to_string();
    assert!(
        error.contains("git_only")
            && error.contains("publish")
            && error.contains("mutually exclusive"),
        "Expected validation error about git_only and publish being mutually exclusive, got: {error}"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_release_creates_tag() {
    use cargo_metadata::semver::Version;

    let context = TestContext::new().await;

    // Configure with git_only = true and publish = false
    let config = r#"
[workspace]
git_only = true
publish = false
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag and update Cargo.toml to match
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Update version and make a new commit
    context.set_package_version(&context.gitea.repo, &Version::parse("0.1.1").unwrap());
    context.run_cargo_check();
    context.push_all_changes("fix: bug fix");

    let crate_name = &context.gitea.repo;
    let expected_tag = "v0.1.1";

    // Verify tag doesn't exist yet
    let is_tag_created = || {
        context.repo.git(&["fetch", "--tags"]).unwrap();
        context.repo.tag_exists(expected_tag).unwrap()
    };
    assert!(!is_tag_created(), "Tag should not exist before release");

    // Run release command
    let outcome = context.run_release().success();

    // Verify JSON output shows the release
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "prs": [],
                "tag": expected_tag,
                "version": "0.1.1",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));

    // Verify the tag was created
    assert!(is_tag_created(), "Tag should exist after release");

    // Verify no packages were published (since publish = false)
    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = context.download_package(dest_dir.path());
    assert!(packages.is_empty());
}

/// Test for <https://github.com/release-plz/release-plz/issues/2594>
/// In `git_only` mode, release-plz should NOT check crates.io for existing packages.
/// This test verifies that a package with a name that exists on crates.io ("log")
/// still gets tagged in `git_only` mode, because the registry check is skipped.
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_release_does_not_check_crates_io() {
    use cargo_metadata::semver::Version;

    // Create workspace with two packages
    let context = TestContext::new_workspace(&["mylib", "mybin"]).await;

    // Rename "mylib" to "log" - a name that definitely exists on crates.io.
    // Without the fix, release-plz would check crates.io and find "log" already published,
    // causing it to skip creating the tag for this package.
    context.set_package_name("mylib", "log");
    context.run_cargo_check();
    context.push_all_changes("rename mylib to log");

    // Configure with git_only = true and publish = false
    let config = r#"
[workspace]
git_only = true
publish = false
"#;
    context.write_release_plz_toml(config);

    // Create initial release tags
    context
        .repo
        .tag("log-v0.1.0", "Release log v0.1.0")
        .unwrap();
    context
        .repo
        .tag("mybin-v0.1.0", "Release mybin v0.1.0")
        .unwrap();

    // Update versions and make a new commit
    context.set_package_version("mylib", &Version::parse("0.1.1").unwrap());
    context.set_package_version("mybin", &Version::parse("0.1.1").unwrap());
    context.run_cargo_check();
    context.push_all_changes("fix: bug fix in both packages");

    let expected_log_tag = "log-v0.1.1";
    let expected_mybin_tag = "mybin-v0.1.1";

    // Verify tags don't exist yet
    let tag_exists = |tag: &str| {
        context.repo.git(&["fetch", "--tags"]).unwrap();
        context.repo.tag_exists(tag).unwrap()
    };
    assert!(
        !tag_exists(expected_log_tag),
        "log tag should not exist before release"
    );
    assert!(
        !tag_exists(expected_mybin_tag),
        "mybin tag should not exist before release"
    );

    // Run release command - this should succeed and create both tags
    // Without the fix, it would only create mybin tag because "log" exists on crates.io
    let outcome = context.run_release().success();

    // Verify JSON output shows both releases
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": "log",
                "prs": [],
                "tag": expected_log_tag,
                "version": "0.1.1",
            },
            {
                "package_name": "mybin",
                "prs": [],
                "tag": expected_mybin_tag,
                "version": "0.1.1",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));

    // Verify BOTH tags were created - this is the key assertion.
    // Before the fix, only mybin-v0.1.1 would be created because "log" exists on crates.io.
    assert!(
        tag_exists(expected_log_tag),
        "log tag should exist after release (git_only should not check crates.io)"
    );
    assert!(
        tag_exists(expected_mybin_tag),
        "mybin tag should exist after release"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_processes_packages_with_publish_false_in_manifest() {
    use cargo_utils::LocalManifest;

    let context = TestContext::new().await;

    // Set publish = false in the Cargo.toml manifest
    let cargo_toml_path = context.repo_dir().join("Cargo.toml");
    let mut cargo_toml = LocalManifest::try_new(&cargo_toml_path).unwrap();
    cargo_toml.data["package"]["publish"] = false.into();
    cargo_toml.write().unwrap();
    context.push_all_changes("chore: set publish = false");

    // Configure git_only = true in release-plz config
    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag
    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr - should succeed even though publish = false in manifest
    context.run_release_pr().success();

    // Verify PR was created with correct version bump (0.1.0 -> 0.1.1)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}
