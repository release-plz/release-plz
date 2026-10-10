use next_version::{NextVersion, VersionUpdater};
use semver::Version;

#[test]
fn no_commits_leave_pre_release_unchanged() {
    let version = Version::parse("1.0.0-alpha.2").unwrap();
    assert_eq!(version.next(std::iter::empty::<&str>()), version);
}

#[test]
fn pre_release_increment_uses_first_commit_not_ignored_by_regex() {
    let version = Version::parse("1.0.0-alpha.2").unwrap();
    let commits = [
        "docs: update readme",
        "fix: correct typo",
        "feat!: change behavior",
    ];
    assert_eq!(
        VersionUpdater::new()
            .with_no_increment_regex("^docs$")
            .unwrap()
            .increment(&version, commits),
        Version::parse("1.0.0-alpha.3").unwrap()
    );
}

#[test]
fn commit_without_semver_prefix_increments_pre_release_version() {
    let commits = ["my change"];
    for (version, expected) in [
        ("0.0.1-alpha.2", "0.0.1-alpha.3"),
        ("1.0.0-alpha.2", "1.0.0-alpha.3"),
    ] {
        assert_eq!(
            Version::parse(version).unwrap().next(commits),
            Version::parse(expected).unwrap()
        );
    }
}

#[test]
fn commit_with_breaking_change_increments_pre_release_version() {
    let commits = ["feat!: break user"];
    let version = Version::parse("1.0.0-alpha.2").unwrap();
    let expected = Version::parse("1.0.0-alpha.3").unwrap();
    assert_eq!(version.next(commits), expected);
}

#[test]
fn dot_1_is_added_to_unversioned_pre_release() {
    let commits = ["feat!: break user"];
    let version = Version::parse("1.0.0-alpha").unwrap();
    let expected = Version::parse("1.0.0-alpha.1").unwrap();
    assert_eq!(version.next(commits), expected);
}

#[test]
fn dot_1_is_added_to_last_identifier_in_pre_release() {
    let commits = ["feat!: break user"];
    let version = Version::parse("1.0.0-beta.1.2").unwrap();
    let expected = Version::parse("1.0.0-beta.1.3").unwrap();
    assert_eq!(version.next(commits), expected);
}

#[test]
fn dot_1_is_added_to_character_identifier_in_pre_release() {
    let commits = ["feat!: break user"];
    let version = Version::parse("1.0.0-beta.1.a").unwrap();
    let expected = Version::parse("1.0.0-beta.1.a.1").unwrap();
    assert_eq!(version.next(commits), expected);
}
