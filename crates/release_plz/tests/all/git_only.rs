use crate::helpers::test_context::TestContext;
use tracing::info;

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_default_prefix() {
    let context = TestContext::new().await;

    // Configure git_only with "v" prefix
    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
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
async fn git_only_with_prefix_and_suffix() {
    let context = TestContext::new().await;

    // Configure with both prefix and suffix
    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "release-"
git_only_release_tag_suffix = "-prod"
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
async fn git_only_no_prefix_no_suffix() {
    let context = TestContext::new().await;

    // Configure with no prefix or suffix (empty strings)
    let config = r#"
[workspace]
git_only = true
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag without prefix/suffix
    context.repo.tag("0.1.0", "Release 0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    context.run_release_pr().success();

    // Verify PR was created
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    // PR title always has "v" prefix regardless of tag format
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_finds_highest_version_tag() {
    info!("running test");

    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
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
git_only_release_tag_prefix = "v"
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
async fn git_only_no_matching_tag_skips_package() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
"#;
    context.write_release_plz_toml(config);

    // Create tag that doesn't match the pattern
    context.repo.tag("release-0.1.0", "Release 0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    let outcome = context.run_release_pr().success();

    // Verify no PR was created (package skipped due to no matching tag)
    outcome.stdout("{\"prs\":[]}\n");
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_no_tags_at_all() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
"#;
    context.write_release_plz_toml(config);

    // Don't create any tags

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Updated README").unwrap();
    context.push_all_changes("fix: update readme");

    // Run release-pr
    let outcome = context.run_release_pr().success();

    // Verify no PR was created
    outcome.stdout("{\"prs\":[]}\n");
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_fix_commit_patch_bump() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
"#;
    context.write_release_plz_toml(config);

    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make a fix commit
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Fixed README").unwrap();
    context.push_all_changes("fix: correct readme");

    context.run_release_pr().success();

    // Verify patch bump (0.1.0 -> 0.1.1)
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_feat_commit_minor_bump() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
features_always_increment_minor = true
"#;
    context.write_release_plz_toml(config);

    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

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
git_only_release_tag_prefix = "v"
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

    // In workspace git_only mode, each package needs a unique prefix to distinguish tags
    let config = r#"
[workspace]
git_only = true

[[package]]
name = "lib1"
git_only_release_tag_prefix = "lib1-v"

[[package]]
name = "lib2"
git_only_release_tag_prefix = "lib2-v"
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

    // Verify lib1 is mentioned (it was changed)
    assert!(
        pr_body.contains("lib1"),
        "Changed package lib1 should be in PR"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_per_package_prefix() {
    let context = TestContext::new_workspace(&["api", "core"]).await;

    // Workspace level: git_only with "v" prefix
    // Package "api": override with "api-v" prefix
    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"

[[package]]
name = "api"
git_only_release_tag_prefix = "api-v"
"#;
    context.write_release_plz_toml(config);

    // Tag the initial state (Cargo.toml already has 0.1.0)
    // In a workspace, each package needs its own tag with its configured prefix
    // api has custom prefix "api-v", core inherits workspace prefix "v"
    context
        .repo
        .tag("api-v0.1.0", "Release api v0.1.0")
        .unwrap();
    context.repo.tag("v0.1.0", "Release core v0.1.0").unwrap();

    // Make changes to api package
    let api_file = context.package_path("api").join("src").join("lib.rs");
    fs_err::write(&api_file, "pub fn api_updated() {}").unwrap();
    context.push_all_changes("feat: update api");

    context.run_release_pr().success();

    // Verify PR is created with correct content
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);

    let pr_body = opened_prs[0].body.as_ref().expect("PR should have body");

    // Verify "api" package is in the PR (it was changed)
    assert!(
        pr_body.contains("api"),
        "Changed package 'api' should be in PR"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_with_lightweight_tags() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
"#;
    context.write_release_plz_toml(config);

    // Create initial release tag as lightweight tag (no message)
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
git_only_release_tag_prefix = "v"
"#;
    context.write_release_plz_toml(config);

    // Create mix of annotated and lightweight tags with proper version updates
    // Tag v0.1.0 as lightweight (Cargo.toml already has 0.1.0 from cargo init)
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
git_only_release_tag_prefix = "v"
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
async fn git_only_multiple_commits_between_releases() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
"#;
    context.write_release_plz_toml(config);

    context.repo.tag("v0.1.0", "Release v0.1.0").unwrap();

    // Make multiple commits
    let readme = context.repo_dir().join("README.md");
    fs_err::write(&readme, "# Fix 1").unwrap();
    context.push_all_changes("fix: first fix");

    fs_err::write(&readme, "# Fix 2").unwrap();
    context.push_all_changes("fix: second fix");

    let new_file = context.repo_dir().join("src").join("new.rs");
    fs_err::write(&new_file, "// New feature").unwrap();
    context.push_all_changes("feat: add new feature");

    fs_err::write(&readme, "# Fix 3").unwrap();
    context.push_all_changes("fix: third fix");

    // Run release-pr
    context.run_release_pr().success();

    // All commits should be included in a single PR
    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    // Feat commit should be included, triggering patch bump
    assert_eq!(opened_prs[0].title, "chore: release v0.1.1");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn git_only_breaking_change() {
    let context = TestContext::new().await;

    let config = r#"
[workspace]
git_only = true
git_only_release_tag_prefix = "v"
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

    let config = r#"
[workspace]
git_only = true

[[package]]
name = "pkg1"
git_only_release_tag_prefix = "pkg1-v"

[[package]]
name = "pkg2"
git_only_release_tag_prefix = "pkg2-v"

[[package]]
name = "pkg3"
git_only_release_tag_prefix = "pkg3-v"
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

    let pr_body = opened_prs[0].body.as_ref().unwrap();
    // Both changed packages should be mentioned
    assert!(
        pr_body.contains("pkg1"),
        "Changed package pkg1 should be in PR"
    );
    assert!(
        pr_body.contains("pkg2"),
        "Changed package pkg2 should be in PR"
    );
}
