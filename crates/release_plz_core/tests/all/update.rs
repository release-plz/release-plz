use cargo_metadata::semver::Version;
use release_plz_core::{CHANGELOG_HEADER, read_package};

use crate::helpers::{comparison_test::ComparisonTest, user_mock};

#[tokio::test]
async fn up_to_date_project_is_not_touched() {
    let comparison_test = ComparisonTest::new().await;

    comparison_test.run_update().await;

    // The update shouldn't have changed anything.
    assert!(comparison_test.are_projects_equal());
}

#[tokio::test]
async fn version_is_updated_when_project_changed() {
    let comparison_test = ComparisonTest::new().await;
    let feature_message = "do awesome stuff";
    user_mock::add_feature(&comparison_test.local_project(), feature_message);

    comparison_test.run_update().await;

    let local_package = read_package(comparison_test.local_project()).unwrap();
    assert_eq!(local_package.version, Version::new(0, 1, 1));
    // Assert: changelog is generated.
    expect_test::expect![[r#"
        # Changelog
        
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.1.1] - 2015-05-15

        ### Added

        - do awesome stuff
    "#]]
    .assert_eq(&comparison_test.local_project_changelog());
}

#[tokio::test]
async fn changelog_is_updated_if_changelog_already_exists() {
    let old_body = r#"
## [0.1.0] - 1970-01-01

### Fixed

- fix important bug
"#;
    let comparison_test = ComparisonTest::new().await;
    let old_changelog = format!("{CHANGELOG_HEADER}{old_body}");
    comparison_test.write_local_project_changelog(&old_changelog);
    let feature_message = "do awesome stuff";
    user_mock::add_feature(&comparison_test.local_project(), feature_message);

    comparison_test.run_update().await;

    let local_package = read_package(comparison_test.local_project()).unwrap();
    assert_eq!(local_package.version, Version::new(0, 1, 1));
    expect_test::expect![[r#"
        # Changelog

        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.1.1] - 2015-05-15

        ### Added

        - do awesome stuff

        ## [0.1.0] - 1970-01-01

        ### Fixed

        - fix important bug
    "#]]
    .assert_eq(&comparison_test.local_project_changelog());
}
