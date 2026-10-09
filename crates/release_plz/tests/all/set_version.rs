use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use release_plz_core::{CHANGELOG_FILENAME, copy_to_temp_dir, fs_utils::Utf8TempDir};

use crate::helpers::test_context::run_set_version;

#[test]
fn set_version_updates_target_dependencies_and_unused_workspace_templates() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    let workspace_path = project_dir.join(CARGO_TOML);
    let workspace = fs_err::read_to_string(&workspace_path).unwrap();
    fs_err::write(
        &workspace_path,
        format!(
            "{workspace}\n[workspace.dependencies]\nunused = {{ package = \"one\", path = \"crates/one\", version = \"0.1.0\" }}\n"
        ),
    )
    .unwrap();
    let two_dir = project_dir.join("crates/two");
    let two_path = two_dir.join(CARGO_TOML);
    let two = fs_err::read_to_string(&two_path).unwrap();
    fs_err::write(
        &two_path,
        format!(
            "{two}\n[target.'cfg(windows)'.build-dependencies]\nrenamed = {{ package = \"one\", path = \"../one\", version = \"0.1.0\" }}\n"
        ),
    )
    .unwrap();

    run_set_version(&project_dir, "one@0.1.1");

    assert_eq!(
        read_manifest(&project_dir)["workspace"]["dependencies"]["unused"]["version"].as_str(),
        Some("0.1.1")
    );
    assert_eq!(
        read_manifest(&two_dir)["target"]["cfg(windows)"]["build-dependencies"]["renamed"]["version"]
            .as_str(),
        Some("0.1.1")
    );
}

#[test]
fn set_version_updates_dependencies_with_metadata_from_original_workspace() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    let project_dir = release_plz_core::fs_utils::canonicalize_utf8(&project_dir).unwrap();
    let two_path = project_dir.join("crates/two/Cargo.toml");
    let two = fs_err::read_to_string(&two_path).unwrap();
    fs_err::write(
        &two_path,
        format!(
            "{two}\n[dev-dependencies]\nrenamed = {{ package = \"one\", path = \"../one\", version = \"0.1.0\" }}\n[target.'cfg(unix)'.build-dependencies]\nrenamed = {{ package = \"one\", path = \"../one\", version = \"0.1.0\" }}\n"
        ),
    )
    .unwrap();
    let metadata = cargo_utils::get_manifest_metadata(&project_dir.join(CARGO_TOML)).unwrap();
    let mut packages: Vec<_> = cargo_utils::workspace_members(&metadata).unwrap().collect();
    let copied = copy_to_temp_dir(&project_dir).unwrap();
    let copied_dir = copied.path().join(project_dir.file_name().unwrap());
    let copied_dir = release_plz_core::fs_utils::canonicalize_utf8(&copied_dir).unwrap();
    // Match release-pr: relocate the manifests while dependency paths in the
    // metadata still refer to the original checkout.
    for package in &mut packages {
        package.manifest_path =
            copied_dir.join(package.manifest_path.strip_prefix(&project_dir).unwrap());
    }
    // The public API also accepts a list that omits the updated package itself.
    for include_updated in [true, false] {
        let packages: Vec<_> = packages
            .iter()
            .filter(|package| include_updated || package.name != "one")
            .collect();
        let version =
            cargo_metadata::semver::Version::new(0, 1, if include_updated { 1 } else { 2 });
        release_plz_core::set_version(
            &packages,
            &copied_dir.join("crates/one"),
            &version,
            &copied_dir.join(CARGO_TOML),
        )
        .unwrap();
        let two = read_manifest(&copied_dir.join("crates/two"));
        assert_eq!(
            two["dev-dependencies"]["renamed"]["version"]
                .as_str()
                .unwrap(),
            version.to_string()
        );
        assert_eq!(
            two["target"]["cfg(unix)"]["build-dependencies"]["renamed"]["version"]
                .as_str()
                .unwrap(),
            version.to_string()
        );
    }
}

#[test]
fn set_version_does_not_rewrite_manifests_without_dependency_changes() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    let unchanged = [
        project_dir.join(CARGO_TOML),
        project_dir.join("crates/two/Cargo.toml"),
    ];
    let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    for manifest in &unchanged {
        std::fs::File::options()
            .write(true)
            .open(manifest)
            .unwrap()
            .set_modified(timestamp)
            .unwrap();
    }

    run_set_version(&project_dir, "one@0.1.1");

    for manifest in unchanged {
        assert_eq!(
            fs_err::metadata(manifest).unwrap().modified().unwrap(),
            timestamp
        );
    }
    assert_eq!(
        read_manifest(&project_dir.join("crates/one"))["package"]["version"].as_str(),
        Some("0.1.1")
    );
}

#[test]
fn set_version_updates_version_in_workspace() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
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
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-package");
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
    let (_temp_dir, project_dir) = copy_fixture("set-version-inherited-workspace");
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
    // Rewriting manifests normalizes CRLF line endings from Windows checkouts to LF.
    assert_eq!(
        fs_err::read_to_string(one_dir.join(CARGO_TOML))
            .unwrap()
            .replace("\r\n", "\n"),
        one_manifest.replace("\r\n", "\n")
    );
    assert_eq!(
        fs_err::read_to_string(three_dir.join(CARGO_TOML))
            .unwrap()
            .replace("\r\n", "\n"),
        three_manifest.replace("\r\n", "\n")
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
    let (_temp_dir, project_dir) = copy_fixture("set-version-inherited-package");
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
    let (_temp_dir, project_dir) = copy_fixture("set-version-without-workspace-version");
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
    let (_temp_dir, project_dir) = copy_fixture("set-version-shared-changelog");
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    let changelog = fs_err::read_to_string(&changelog_path).unwrap();

    run_set_version(&project_dir, "1.2.30");

    assert_eq!(
        fs_err::read_to_string(changelog_path).unwrap(),
        changelog.replace("1.2.3", "1.2.30")
    );
}

// Keep the returned temporary directory alive for the duration of the test.
fn copy_fixture(name: &str) -> (Utf8TempDir, Utf8PathBuf) {
    let fixture_dir = Utf8Path::new("../../tests/fixtures").join(name);
    let temp_dir = copy_to_temp_dir(&fixture_dir).unwrap();
    let project_dir = temp_dir.path().join(name);
    (temp_dir, project_dir)
}

fn read_manifest(directory: &Utf8Path) -> toml_edit::DocumentMut {
    fs_err::read_to_string(directory.join(CARGO_TOML))
        .unwrap()
        .parse()
        .unwrap()
}
