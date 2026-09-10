use cargo_metadata::camino::Utf8Path;
use cargo_utils::CARGO_TOML;
use release_plz_core::{CHANGELOG_FILENAME, copy_to_temp_dir};

use crate::helpers::test_context::run_set_version;

#[test]
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
    expect_test::expect![[r"
        # Changelog
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.1.1] - 2024-05-16

        ### Other
        - stuff in crate one
    "]]
    .assert_eq(
        &fs_err::read_to_string(one_changelog)
            .unwrap()
            .replace("\r\n", "\n"),
    );

    let two_changelog = two_dir.join(CHANGELOG_FILENAME);
    expect_test::expect![[r"
        # Changelog
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.3.0] - 2024-05-16

        ### Other
        - stuff in crate two
    "]]
    .assert_eq(
        &fs_err::read_to_string(two_changelog)
            .unwrap()
            .replace("\r\n", "\n"),
    );

    let workspace_lock = project_dir.join("Cargo.lock");
    let workspace_lock = fs_err::read_to_string(workspace_lock).unwrap();
    assert!(workspace_lock.contains("name = \"one\"\nversion = \"0.1.1\""));
    assert!(workspace_lock.contains("name = \"two\"\nversion = \"0.3.0\""));
}

#[test]
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

        [workspace]
    "#]]
    .assert_eq(&fs_err::read_to_string(manifest).unwrap());

    let changelog = project_dir.join(CHANGELOG_FILENAME);
    expect_test::expect![[r"
        # Changelog
        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [0.1.1] - 2024-05-16

        ### Other
        - stuff in crate
    "]]
    .assert_eq(
        &fs_err::read_to_string(changelog)
            .unwrap()
            .replace("\r\n", "\n"),
    );

    let lockfile = project_dir.join("Cargo.lock");
    let lockfile = fs_err::read_to_string(lockfile).unwrap();
    assert!(lockfile.contains("name = \"set-version-in-package\""));
    assert!(lockfile.contains("version = \"0.1.1\""));
}

#[test]
fn set_version_updates_inherited_workspace_version() {
    let fixture_dir = Utf8Path::new("../../tests/fixtures/set-version-inherited-workspace");
    let dest_dir = copy_to_temp_dir(fixture_dir).unwrap();
    let project_dir = dest_dir.path().join("set-version-inherited-workspace");
    let one_dir = project_dir.join("crates/one");
    let two_dir = project_dir.join("crates/two");
    let three_dir = project_dir.join("crates/three");
    let changelog = fs_err::read_to_string(project_dir.join(CHANGELOG_FILENAME)).unwrap();
    let two_changelog = fs_err::read_to_string(two_dir.join(CHANGELOG_FILENAME)).unwrap();
    let one_manifest = fs_err::read_to_string(one_dir.join(CARGO_TOML)).unwrap();
    let three_manifest = fs_err::read_to_string(three_dir.join(CARGO_TOML)).unwrap();

    run_set_version(&project_dir, "1.2.3");

    let workspace = read_manifest(&project_dir);
    assert_eq!(
        workspace["workspace"]["package"]["version"].as_str(),
        Some("1.2.3")
    );
    assert_eq!(
        workspace["workspace"]["dependencies"]["three"]["version"].as_str(),
        Some("=1.2.3")
    );
    assert_eq!(
        fs_err::read_to_string(one_dir.join(CARGO_TOML)).unwrap(),
        one_manifest
    );
    assert_eq!(
        fs_err::read_to_string(three_dir.join(CARGO_TOML)).unwrap(),
        three_manifest
    );
    let two = read_manifest(&two_dir);
    assert_eq!(two["package"]["version"].as_str(), Some("0.2.0"));
    assert_eq!(
        two["dependencies"]["one"]["version"].as_str(),
        Some("=1.2.3")
    );
    for path in [
        project_dir.join(CHANGELOG_FILENAME),
        three_dir.join(CHANGELOG_FILENAME),
    ] {
        assert_eq!(
            fs_err::read_to_string(path).unwrap(),
            changelog.replace("0.1.0", "1.2.3")
        );
    }
    assert_eq!(
        fs_err::read_to_string(two_dir.join(CHANGELOG_FILENAME)).unwrap(),
        two_changelog
    );
    let lockfile = fs_err::read_to_string(project_dir.join("Cargo.lock")).unwrap();
    for (name, version) in [("one", "1.2.3"), ("three", "1.2.3"), ("two", "0.2.0")] {
        assert!(lockfile.contains(&format!("name = \"{name}\"\nversion = \"{version}\"")));
    }
}

#[test]
fn set_version_preserves_inheritance_for_single_package() {
    let fixture_dir = Utf8Path::new("../../tests/fixtures/set-version-inherited-package");
    let dest_dir = copy_to_temp_dir(fixture_dir).unwrap();
    let project_dir = dest_dir.path().join("set-version-inherited-package");
    // Also support projects without a lockfile.
    assert!(!project_dir.join("Cargo.lock").exists());

    run_set_version(&project_dir, "1.2.3");

    let manifest = read_manifest(&project_dir);
    assert_eq!(
        manifest["workspace"]["package"]["version"].as_str(),
        Some("1.2.3")
    );
    assert_eq!(
        manifest["package"]["version"]["workspace"].as_bool(),
        Some(true)
    );
    assert!(
        fs_err::read_to_string(project_dir.join(CHANGELOG_FILENAME))
            .unwrap()
            .contains("## [1.2.3]")
    );
}

#[test]
fn set_version_requires_package_names_without_workspace_version() {
    let fixture_dir = Utf8Path::new("../../tests/fixtures/set-version-without-workspace-version");
    let dest_dir = copy_to_temp_dir(fixture_dir).unwrap();
    let project_dir = dest_dir
        .path()
        .join("set-version-without-workspace-version");
    let original_manifest = fs_err::read_to_string(project_dir.join(CARGO_TOML)).unwrap();
    let output = crate::helpers::cmd::release_plz_cmd(Utf8Path::new("target"))
        .current_dir(&project_dir)
        .args(["set-version", "1.2.3"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Please specify which package you want to update"),
        "{stderr}"
    );
    assert_eq!(
        fs_err::read_to_string(project_dir.join(CARGO_TOML)).unwrap(),
        original_manifest
    );
}

#[test]
fn set_version_updates_shared_workspace_changelog_once() {
    let fixture_dir = Utf8Path::new("../../tests/fixtures/set-version-shared-changelog");
    let dest_dir = copy_to_temp_dir(fixture_dir).unwrap();
    let project_dir = dest_dir.path().join("set-version-shared-changelog");
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    let changelog = fs_err::read_to_string(&changelog_path).unwrap();

    run_set_version(&project_dir, "1.2.30");

    assert_eq!(
        fs_err::read_to_string(changelog_path).unwrap(),
        changelog.replace("1.2.3", "1.2.30")
    );
}

fn read_manifest(directory: &Utf8Path) -> toml_edit::DocumentMut {
    fs_err::read_to_string(directory.join(CARGO_TOML))
        .unwrap()
        .parse()
        .unwrap()
}
