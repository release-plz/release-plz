use cargo_metadata::camino::Utf8Path;
use cargo_utils::CARGO_TOML;
use release_plz_core::{CHANGELOG_FILENAME, copy_to_temp_dir};

use crate::helpers::test_context::run_set_version;

#[test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
fn set_version_updates_version_in_workspace() {
    let fixture_dir = Utf8Path::new("../../tests/fixtures/set-version-in-workspace");
    assert!(fixture_dir.is_dir());
    let dest_dir = copy_to_temp_dir(fixture_dir).unwrap();
    let project_dir = dest_dir.path().join("set-version-in-workspace");
    run_set_version(&project_dir, "one@0.1.1 two@0.3.0");

    let crates_dir = project_dir.join("crates");
    let one_dir = crates_dir.join("one");
    let two_dir = crates_dir.join("two");

    let one_manifest = one_dir.join(CARGO_TOML);
    expect_test::expect![[r#"
        [package]
        name = "one"
        version = "0.1.1"
        edition = "2024"

        [dependencies]
    "#]]
    .assert_eq(&fs_err::read_to_string(one_manifest).unwrap());

    let two_manifest = two_dir.join(CARGO_TOML);
    expect_test::expect![[r#"
        [package]
        name = "two"
        version = "0.3.0"
        edition = "2024"

        [dependencies]
    "#]]
    .assert_eq(&fs_err::read_to_string(two_manifest).unwrap());

    let one_changelog = project_dir.join(CHANGELOG_FILENAME);
    expect_test::expect![[r#"
        # Changelog
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.1.1] - 2024-05-16

        ### Other
        - stuff in crate one
    "#]]
    .assert_eq(&fs_err::read_to_string(one_changelog).unwrap());

    let two_changelog = two_dir.join(CHANGELOG_FILENAME);
    expect_test::expect![[r#"
        # Changelog
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.3.0] - 2024-05-16

        ### Other
        - stuff in crate two
    "#]]
    .assert_eq(&fs_err::read_to_string(two_changelog).unwrap());
}

#[test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
fn set_version_updates_version_in_package() {
    let fixture_dir = Utf8Path::new("../../tests/fixtures/set-version-in-package");
    assert!(fixture_dir.is_dir());
    let dest_dir = copy_to_temp_dir(fixture_dir).unwrap();
    let project_dir = dest_dir.path().join("set-version-in-package");
    // There's a single crate in this project, so we don't need to specify the package name.
    run_set_version(&project_dir, "0.1.1");

    let manifest = project_dir.join(CARGO_TOML);
    expect_test::expect![[r#"
        [package]
        name = "set-version-in-package"
        version = "0.1.1"

        [dependencies]
    "#]]
    .assert_eq(&fs_err::read_to_string(manifest).unwrap());

    let changelog = project_dir.join(CHANGELOG_FILENAME);
    expect_test::expect![[r#"
        # Changelog
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.1.1] - 2024-05-16

        ### Other
        - stuff in crate
    "#]]
    .assert_eq(&fs_err::read_to_string(changelog).unwrap());
}
