use next_version::{NextVersion, VersionUpdater};
use semver::Version;

#[test]
fn commit_without_semver_prefix_increments_patch_version() {
    let commits = ["my change"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 4));
}

#[test]
fn commit_with_fix_semver_prefix_increments_patch_version() {
    let commits = ["my change", "fix: serious bug"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 4));
}

#[test]
fn commit_with_feat_semver_prefix_increments_patch_version() {
    let commits = ["feat: make coffe"];
    let version = Version::new(1, 3, 3);
    assert_eq!(version.next(commits), Version::new(1, 4, 0));
}

#[test]
fn commit_with_feat_semver_prefix_increments_patch_version_when_major_is_zero() {
    let commits = ["feat: make coffee"];
    let version = Version::new(0, 2, 3);
    assert_eq!(version.next(commits), Version::new(0, 2, 4));
}

#[test]
fn commit_with_feat_semver_prefix_increments_minor_version_when_major_is_zero() {
    let commits = ["feat: make coffee"];
    let version = Version::new(0, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_features_always_increment_minor(true)
            .with_breaking_always_increment_major(false)
            .increment(&version, commits),
        Version::new(0, 3, 0)
    );
}

#[test]
fn commit_with_breaking_change_increments_major_version() {
    let commits = ["feat!: break user"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(2, 0, 0));
}

#[test]
fn commit_with_breaking_change_increments_minor_version_when_major_is_zero() {
    let commits = ["feat!: break user"];
    let version = Version::new(0, 2, 3);
    assert_eq!(version.next(commits), Version::new(0, 3, 0));
}

#[test]
fn commit_with_breaking_change_increments_major_version_when_major_is_zero() {
    let commits = ["feat!: break user"];
    let version = Version::new(0, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_features_always_increment_minor(false)
            .with_breaking_always_increment_major(true)
            .increment(&version, commits),
        Version::new(1, 0, 0)
    );
}

#[test]
fn commit_with_custom_major_increment_regex_increments_major_version() {
    let commits = ["major: some changes"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_custom_major_increment_regex("another|major")
            .unwrap()
            .increment(&version, commits),
        Version::new(2, 0, 0)
    );
}

#[test]
fn commit_with_custom_minor_increment_regex_increments_minor_version() {
    let commits = ["minor: some changes"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_custom_minor_increment_regex("^minor")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 3, 0)
    );
}

#[test]
fn non_conventional_commit_with_custom_minor_increment_regex_increments_minor_version() {
    let commits = ["Some non-conventional commit with minor keyword"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_custom_minor_increment_regex("minor")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 3, 0)
    );
}

#[test]
fn non_conventional_commit_with_custom_major_increment_regex_increments_major_version() {
    let commits = ["A commit that mentions BREAKING in the message"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_custom_major_increment_regex("BREAKING")
            .unwrap()
            .increment(&version, commits),
        Version::new(2, 0, 0)
    );
}

#[test]
fn conventional_commit_with_matching_description_does_not_trigger_custom_regex() {
    // The word "minor" appears in the description, but not in the type
    // For conventional commits, only the type should be checked
    let commits = ["fix: a minor bug"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_custom_minor_increment_regex("minor")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 2, 4) // Patch, not minor
    );
}

#[test]
fn commit_with_no_increment_regex_does_not_increment_version() {
    let commits = ["docs: update readme"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_no_increment_regex("^docs$")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 2, 3)
    );
}

#[test]
fn no_increment_regex_filters_matching_commits_but_keeps_other_bumps() {
    let commits = ["docs: update readme", "feat: make coffee"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_no_increment_regex("^docs$")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 3, 0)
    );
}

#[test]
fn non_conventional_commit_with_no_increment_regex_does_not_increment_version() {
    let commits = ["chore only: skip release"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_no_increment_regex("skip release")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 2, 3)
    );
}

#[test]
fn no_increment_regex_does_not_match_conventional_commit_description() {
    let commits = ["fix: skip release"];
    let version = Version::new(1, 2, 3);
    assert_eq!(
        VersionUpdater::new()
            .with_no_increment_regex("skip release")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 2, 4)
    );
}

#[test]
fn no_increment_regex_skips_prerelease_increment() {
    let commits = ["docs: update readme"];
    let version = Version::parse("1.2.3-alpha.1").unwrap();
    assert_eq!(
        VersionUpdater::new()
            .with_no_increment_regex("^docs$")
            .unwrap()
            .increment(&version, commits),
        version
    );
}

#[test]
fn commit_with_scope() {
    let commits = ["feat(my_scope)!: this is a test commit"];
    let version = Version::new(1, 0, 0);
    assert_eq!(version.next(commits), Version::new(2, 0, 0));
}

#[test]
fn commit_with_scope_whitespace() {
    let commits = ["feat(my scope)!: this is a test commit"];
    let version = Version::new(1, 0, 0);
    assert_eq!(version.next(commits), Version::new(2, 0, 0));
}

#[test]
fn commit_with_scope_minor() {
    let commits = ["feat(my scope): this is a test commit"];
    let version = Version::new(1, 0, 0);
    assert_eq!(version.next(commits), Version::new(1, 1, 0));
}
