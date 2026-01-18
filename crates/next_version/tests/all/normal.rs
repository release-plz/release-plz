use next_version::{NextVersion, VersionUpdater};
use semver::Version;

#[test]
fn commit_without_semver_prefix_does_not_increment_version() {
    // Non-conventional commits should not trigger version bumps
    let commits = ["my change"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
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
            .with_custom_minor_increment_regex("minor")
            .unwrap()
            .increment(&version, commits),
        Version::new(1, 3, 0)
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

#[test]
fn ci_commits_do_not_trigger_version_bump() {
    let commits = ["ci: update github actions"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}

#[test]
fn docs_commits_do_not_trigger_version_bump() {
    let commits = ["docs: update README"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}

#[test]
fn test_commits_do_not_trigger_version_bump() {
    let commits = ["test: add unit tests"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}

#[test]
fn chore_commits_do_not_trigger_version_bump() {
    let commits = ["chore: update dependencies"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}

#[test]
fn style_commits_do_not_trigger_version_bump() {
    let commits = ["style: format code"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}

#[test]
fn refactor_commits_do_not_trigger_version_bump() {
    let commits = ["refactor: simplify logic"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}

#[test]
fn perf_commits_do_not_trigger_version_bump() {
    let commits = ["perf: optimize queries"];
    let version = Version::new(1, 2, 3);
    assert_eq!(version.next(commits), Version::new(1, 2, 3));
}
