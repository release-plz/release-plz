use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use release_plz_core::{CHANGELOG_FILENAME, copy_to_temp_dir, fs_utils::Utf8TempDir};

use crate::helpers::test_context::run_set_version;

#[test]
fn set_version_updates_root_package_and_workspace_dependencies() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    let workspace_path = project_dir.join(CARGO_TOML);
    let workspace = fs_err::read_to_string(&workspace_path).unwrap();
    fs_err::write(
        &workspace_path,
        format!(
            r#"{workspace}
[package]
name = "root-app"
version = "0.1.0"
edition = "2024"

[dependencies]
renamed = {{ package = "one", path = "crates/one", version = "0.1.0" }}

[target.'cfg(windows)'.build-dependencies]
renamed.workspace = true

[workspace.dependencies]
renamed = {{ package = "one", path = "crates/one", version = "0.1.0" }}
"#
        ),
    )
    .unwrap();
    fs_err::create_dir_all(project_dir.join("src")).unwrap();
    fs_err::write(project_dir.join("src/lib.rs"), "").unwrap();

    run_set_version(&project_dir, "one@0.1.1");

    let root = read_manifest(&project_dir);
    assert_eq!(root["package"]["version"].as_str(), Some("0.1.0"));
    assert_eq!(
        root["dependencies"]["renamed"]["version"].as_str(),
        Some("0.1.1")
    );
    assert_eq!(
        root["workspace"]["dependencies"]["renamed"]["version"].as_str(),
        Some("0.1.1")
    );
    assert_eq!(
        root["target"]["cfg(windows)"]["build-dependencies"]["renamed"]["workspace"].as_bool(),
        Some(true)
    );
}

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
fn set_version_rejects_duplicate_package_config() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-package");
    fs_err::write(
        project_dir.join("release-plz.toml"),
        r#"
[[package]]
name = "set-version-in-package"

[[package]]
name = "set-version-in-package"
"#,
    )
    .unwrap();

    let stderr = run_set_version_failure(&project_dir, "0.1.1");
    assert!(
        stderr.contains("package `set-version-in-package` is configured more than once"),
        "{stderr}"
    );
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
    let stderr = run_set_version_failure(&project_dir, "1.2.3");
    assert!(
        stderr.contains("Please specify which package you want to update"),
        "{stderr}"
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

#[test]
fn set_version_resolves_changelog_path_from_workspace_root() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    // Run from outside the workspace: the `changelog_path = "./CHANGELOG.md"` of `one`
    // must still point to the workspace changelog, not to the current directory.
    run_set_version(
        project_dir.parent().unwrap(),
        "--manifest-path set-version-in-workspace/Cargo.toml --config set-version-in-workspace/release-plz.toml one@0.1.1",
    );

    let changelog = fs_err::read_to_string(project_dir.join(CHANGELOG_FILENAME)).unwrap();
    assert!(changelog.contains("## [0.1.1] - 2024-05-16"), "{changelog}");
}

#[test]
fn set_version_uses_workspace_changelog_path() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    // `one` has no `[[package]]` entry, so it uses the workspace changelog path,
    // while `two` overrides it.
    fs_err::write(
        project_dir.join("release-plz.toml"),
        r#"
[workspace]
changelog_path = "./CHANGELOG.md"

[[package]]
name = "two"
changelog_path = "crates/two/CHANGELOG.md"
"#,
    )
    .unwrap();

    run_set_version(&project_dir, "one@0.1.1 two@0.3.0");

    let one_changelog = fs_err::read_to_string(project_dir.join(CHANGELOG_FILENAME)).unwrap();
    assert!(
        one_changelog.contains("## [0.1.1] - 2024-05-16"),
        "{one_changelog}"
    );
    let two_changelog =
        fs_err::read_to_string(project_dir.join("crates/two").join(CHANGELOG_FILENAME)).unwrap();
    assert!(
        two_changelog.contains("## [0.3.0] - 2024-05-16"),
        "{two_changelog}"
    );
}

#[test]
fn set_version_skips_changelog_when_changelog_update_is_disabled() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    fs_err::write(
        project_dir.join("release-plz.toml"),
        "[workspace]\nchangelog_update = false\n",
    )
    .unwrap();
    let two_dir = project_dir.join("crates/two");
    let changelog_path = two_dir.join(CHANGELOG_FILENAME);
    let changelog = fs_err::read_to_string(&changelog_path).unwrap();

    run_set_version(&project_dir, "two@0.3.0");

    assert_eq!(fs_err::read_to_string(changelog_path).unwrap(), changelog);
    assert_eq!(
        read_manifest(&two_dir)["package"]["version"].as_str(),
        Some("0.3.0")
    );
}

#[test]
fn set_version_updates_package_releases_in_shared_changelog() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    fs_err::write(
        project_dir.join("release-plz.toml"),
        "[workspace]\nchangelog_path = \"./CHANGELOG.md\"\n",
    )
    .unwrap();
    // Release titles of the template in the "single changelog" guide of the website.
    // The release of `one` is below the newer release of `two`,
    // whose link contains the current version of `one` (0.1.0).
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    fs_err::write(
        &changelog_path,
        "\
# Changelog

## [Unreleased]

## `two` - [0.2.0](https://github.com/owner/repo/compare/two-v0.1.0...two-v0.2.0) - 2024-05-17

### Added
- new feature

## `one` - [0.1.0](https://github.com/owner/repo/releases/tag/one-v0.1.0) - 2024-05-16

### Other
- initial release
",
    )
    .unwrap();

    run_set_version(&project_dir, "one@0.1.1 two@0.3.0");

    expect_test::expect![[r"
        # Changelog

        ## [Unreleased]

        ## `two` - [0.3.0](https://github.com/owner/repo/compare/two-v0.1.0...two-v0.3.0) - 2024-05-17

        ### Added
        - new feature

        ## `one` - [0.1.1](https://github.com/owner/repo/releases/tag/one-v0.1.1) - 2024-05-16

        ### Other
        - initial release
    "]]
    .assert_eq(&fs_err::read_to_string(changelog_path).unwrap());
}

#[test]
fn set_version_updates_package_release_without_backticks() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    // Like the changelog of ratatui: release titles start with the package name
    // without backticks, and older releases have plain titles.
    // `one-macros` is listed first and has the same version as `one`,
    // so a title must be ignored if the package name is only its prefix.
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    let changelog = "\
# Changelog

## one-macros - [0.1.0](https://github.com/owner/repo/compare/one-macros-v0.0.1...one-macros-v0.1.0) - 2025-06-30

- macros change

## one - [0.1.0](https://github.com/owner/repo/compare/one-v0.0.9...one-v0.1.0) - 2025-06-30

- one change

## [0.0.9](https://github.com/owner/repo/releases/tag/v0.0.9) - 2025-01-14

- old release
";
    fs_err::write(&changelog_path, changelog).unwrap();

    run_set_version(&project_dir, "one@0.1.1");

    assert_eq!(
        fs_err::read_to_string(changelog_path).unwrap(),
        changelog.replace(
            "## one - [0.1.0](https://github.com/owner/repo/compare/one-v0.0.9...one-v0.1.0)",
            "## one - [0.1.1](https://github.com/owner/repo/compare/one-v0.0.9...one-v0.1.1)"
        )
    );
}

#[test]
fn set_version_updates_each_package_release_in_shared_workspace_changelog() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-shared-changelog");
    // Both packages inherit the workspace version, but each one has its own release.
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    let changelog = "\
# Changelog

## `two` - [1.2.3](https://github.com/owner/repo/compare/two-v1.2.2...two-v1.2.3) - 2024-05-16

## `one` - [1.2.3](https://github.com/owner/repo/compare/one-v1.2.2...one-v1.2.3) - 2024-05-16
";
    fs_err::write(&changelog_path, changelog).unwrap();

    run_set_version(&project_dir, "1.2.30");

    assert_eq!(
        fs_err::read_to_string(changelog_path).unwrap(),
        changelog.replace("1.2.3", "1.2.30")
    );
}

#[test]
fn set_version_updates_shared_release_of_packages_set_to_the_same_version() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-shared-changelog");
    // Both packages are at 1.2.3 and share the release `## [1.2.3]`.
    // Renaming it once per package used to produce `## [1.2.300]`.
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    let changelog = fs_err::read_to_string(&changelog_path).unwrap();

    run_set_version(&project_dir, "one@1.2.30 two@1.2.30");

    assert_eq!(
        fs_err::read_to_string(changelog_path).unwrap(),
        changelog.replace("1.2.3", "1.2.30")
    );
}

#[test]
fn set_version_fails_if_packages_sharing_a_release_get_different_versions() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-shared-changelog");
    // Both packages are at 1.2.3 and share the release `## [1.2.3]`,
    // which can't be renamed to both versions.
    let files = [
        project_dir.join(CHANGELOG_FILENAME),
        project_dir.join("crates/one").join(CARGO_TOML),
        project_dir.join("crates/two").join(CARGO_TOML),
    ];
    let original_files = files.each_ref().map(|f| fs_err::read_to_string(f).unwrap());

    let stderr = run_set_version_failure(&project_dir, "one@1.2.30 two@1.2.31");

    assert!(
        stderr.contains("can't set `one` to 1.2.30 and `two` to 1.2.31")
            && stderr.contains("[1.2.3](https://example.com/releases/v1.2.3) - 2024-05-16")
            && stderr.contains("CHANGELOG.md"),
        "{stderr}"
    );
    for (file, original) in files.iter().zip(original_files) {
        assert_eq!(fs_err::read_to_string(file).unwrap(), original, "{file}");
    }
}

#[test]
fn set_version_replaces_only_whole_versions_in_release_title() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-package");
    // The link starts from a pre-release of the current version (0.1.0),
    // whose version contains the current one but must stay unchanged.
    let changelog_path = project_dir.join(CHANGELOG_FILENAME);
    fs_err::write(
        &changelog_path,
        "\
# Changelog

## [0.1.0](https://github.com/owner/repo/compare/v0.1.0-alpha.1...v0.1.0) - 2024-05-16
",
    )
    .unwrap();

    run_set_version(&project_dir, "0.1.1");

    expect_test::expect![[r"
        # Changelog

        ## [0.1.1](https://github.com/owner/repo/compare/v0.1.0-alpha.1...v0.1.1) - 2024-05-16
    "]]
    .assert_eq(&fs_err::read_to_string(changelog_path).unwrap());
}

#[test]
fn set_version_warns_when_last_release_is_not_the_current_version() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    // `one` has no release in its changelog.
    let one_changelog_path = project_dir.join(CHANGELOG_FILENAME);
    let one_changelog = "# Changelog\n\n## [Unreleased]\n";
    fs_err::write(&one_changelog_path, one_changelog).unwrap();
    // The last release of `two` is older than its current version (0.2.0).
    let two_dir = project_dir.join("crates/two");
    let two_changelog_path = two_dir.join(CHANGELOG_FILENAME);
    let two_changelog = "# Changelog\n\n## [0.1.0] - 2024-05-16\n";
    fs_err::write(&two_changelog_path, two_changelog).unwrap();

    let stderr = run_set_version_with_warnings(&project_dir, "one@0.1.1 two@0.3.0");

    assert!(
        stderr.contains("one: no release found in changelog"),
        "{stderr}"
    );
    assert!(
        stderr.contains("two: the last release in changelog")
            && stderr.contains("is 0.1.0, not the current version 0.2.0"),
        "{stderr}"
    );
    // The versions are updated anyway: there's no release to rename in the changelogs.
    assert_eq!(
        read_manifest(&project_dir.join("crates/one"))["package"]["version"].as_str(),
        Some("0.1.1")
    );
    assert_eq!(
        read_manifest(&two_dir)["package"]["version"].as_str(),
        Some("0.3.0")
    );
    assert_eq!(
        fs_err::read_to_string(one_changelog_path).unwrap(),
        one_changelog
    );
    assert_eq!(
        fs_err::read_to_string(two_changelog_path).unwrap(),
        two_changelog
    );
}

#[test]
fn set_version_warns_when_changelog_is_missing() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-package");
    fs_err::remove_file(project_dir.join(CHANGELOG_FILENAME)).unwrap();

    let stderr = run_set_version_with_warnings(&project_dir, "0.1.1");

    assert!(stderr.contains("changelog not found at"), "{stderr}");
    assert_eq!(
        read_manifest(&project_dir)["package"]["version"].as_str(),
        Some("0.1.1")
    );
}

#[test]
fn set_version_does_not_write_files_if_a_changelog_is_invalid() {
    let (_temp_dir, project_dir) = copy_fixture("set-version-in-workspace");
    // `two` is processed after `one`, so the files of `one` must not be
    // written before the changelogs of all packages are validated.
    fs_err::write(
        project_dir.join("crates/two").join(CHANGELOG_FILENAME),
        b"\xff not UTF-8",
    )
    .unwrap();
    let files = [
        project_dir.join("crates/one").join(CARGO_TOML),
        project_dir.join("crates/two").join(CARGO_TOML),
        project_dir.join(CHANGELOG_FILENAME),
    ];
    let original_files = files.each_ref().map(|f| fs_err::read_to_string(f).unwrap());

    let stderr = run_set_version_failure(&project_dir, "one@0.1.1 two@0.3.0");

    for (file, original) in files.iter().zip(original_files) {
        assert_eq!(fs_err::read_to_string(file).unwrap(), original, "{file}");
    }
    assert!(stderr.contains("failed to read changelog"), "{stderr}");
}

// Keep the returned temporary directory alive for the duration of the test.
fn copy_fixture(name: &str) -> (Utf8TempDir, Utf8PathBuf) {
    let fixture_dir = Utf8Path::new("../../tests/fixtures").join(name);
    let temp_dir = copy_to_temp_dir(&fixture_dir).unwrap();
    let project_dir = temp_dir.path().join(name);
    (temp_dir, project_dir)
}

// Returns the stderr of the failed command.
fn run_set_version_failure(project_dir: &Utf8Path, args: &str) -> String {
    let manifest_path = project_dir.join(CARGO_TOML);
    let original_manifest = fs_err::read_to_string(&manifest_path).unwrap();
    let output = crate::helpers::cmd::release_plz_cmd(Utf8Path::new("target"))
        .current_dir(project_dir)
        .arg("set-version")
        .args(args.split(' '))
        .assert()
        .failure();
    // The command must fail before updating the manifest.
    assert_eq!(
        fs_err::read_to_string(&manifest_path).unwrap(),
        original_manifest
    );
    String::from_utf8_lossy(&output.get_output().stderr).into_owned()
}

// Returns the stderr of the successful command, which includes warnings.
fn run_set_version_with_warnings(project_dir: &Utf8Path, args: &str) -> String {
    let output = crate::helpers::cmd::release_plz_cmd(Utf8Path::new("target"))
        .current_dir(project_dir)
        .env("RELEASE_PLZ_LOG", "warn")
        .arg("set-version")
        .args(args.split(' '))
        .assert()
        .success();
    String::from_utf8_lossy(&output.get_output().stderr).into_owned()
}

fn read_manifest(directory: &Utf8Path) -> toml_edit::DocumentMut {
    fs_err::read_to_string(directory.join(CARGO_TOML))
        .unwrap()
        .parse()
        .unwrap()
}
