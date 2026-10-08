use next_version::NextVersion;
use semver::Version;

#[test]
fn commit_without_semver_prefix_increments_pre_release_version() {
    let commits = ["my change"];
    let version = Version::parse("1.0.0-alpha.2").unwrap();
    let expected = Version::parse("1.0.0-alpha.3").unwrap();
    assert_eq!(version.next(commits), expected);
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
