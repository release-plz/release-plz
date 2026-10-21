use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::test_context::TestContext;

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_info_contains_prs_in_changelog() {
    let context = TestContext::new().await;

    let changelog = r"
# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.5](https://github.com/release-plz/release-plz/compare/git_cmd-v0.6.4...git_cmd-v0.6.5) - 2024-05-05

### Other
- add clippy lints ([#1439](https://github.com/release-plz/release-plz/pull/1439))
- improve uncommitted changes error message ([#1434](https://github.com/release-plz/release-plz/pull/1434))
";
    context.write_changelog(changelog);

    let crate_name = &context.gitea.repo;

    let outcome = context.run_release().success();
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "tag": "v0.1.0",
                "version": "0.1.0",
                "prs": [
                    {
                        "html_url":"https://github.com/release-plz/release-plz/pull/1439",
                        "number":1439
                    },
                    {
                        "html_url":"https://github.com/release-plz/release-plz/pull/1434",
                        "number":1434
                    }
                ],
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_releases_a_new_project_with_custom_tag_name() {
    let context = TestContext::new().await;

    let config = r#"
    [workspace]
    git_tag_name = "{{ package }}--{{ version }}"
    "#;
    context.write_release_plz_toml(config);

    let crate_name = &context.gitea.repo;

    let expected_tag = format!("{crate_name}--0.1.0");
    let is_tag_created = || {
        context.repo.git(&["fetch", "--tags"]).unwrap();
        context.repo.tag_exists(&expected_tag).unwrap()
    };

    assert!(!is_tag_created());

    let outcome = context.run_release().success();
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "prs": [],
                "tag": expected_tag,
                "version": "0.1.0",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));

    assert!(is_tag_created());
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_does_not_release_a_new_project_if_release_always_is_false() {
    let context = TestContext::new().await;

    let config = r"
    [workspace]
    release_always = false
    ";
    context.write_release_plz_toml(config);

    // Running `release` doesn't release the project
    // because the last commit doesn't belong to a release PR.
    let outcome = context.run_release().success();
    outcome.stdout("{\"releases\":[]}\n");

    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = context.download_package(dest_dir.path()).await;
    assert!(packages.is_empty());

    // TODO: Gitea doesn't detect associated PRs. I don't know why.
    // context.run_release_pr().success();
    // context.merge_release_pr().unwrap();

    // // Running `release` releases the project
    // // because the last commit belongs to a release PR.
    // let outcome = context.run_release().success();
    // outcome.success();
    // assert_eq!(packages().len(), 1);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_releases_a_new_project_with_custom_release() {
    let context = TestContext::new().await;

    let config = r#"
    [workspace]
    git_release_name = "{{ package }}--{{ version }}"
    git_release_body = "Welcome to this new release!\n{{ changelog }}"
    "#;
    context.write_release_plz_toml(config);

    let crate_name = &context.gitea.repo;

    let expected_tag = "v0.1.0";
    let expected_release = format!("{crate_name}--0.1.0");

    context.run_release().success();

    let gitea_release = context.gitea.get_gitea_release(expected_tag).await;
    assert_eq!(gitea_release.name, expected_release);
    // There's no changelog, so {{ changelog }} should be empty
    expect_test::expect!["Welcome to this new release!\n"].assert_eq(&gitea_release.body);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_releases_after_release_pr_merged() {
    let context = TestContext::new().await;

    let config = r#"
    [workspace]
    git_release_name = "{{ package }}--{{ version }}"
    git_release_body = "Welcome to this new release! Changes:\n\n{{ changelog }}"
    "#;
    context.write_release_plz_toml(config);

    context.run_release_pr().success();
    context.merge_release_pr().await;

    let crate_name = &context.gitea.repo;

    let expected_tag = "v0.1.0";
    let expected_release = format!("{crate_name}--0.1.0");

    context.run_release().success();

    let gitea_release = context.gitea.get_gitea_release(expected_tag).await;
    assert_eq!(gitea_release.name, expected_release);
    expect_test::expect![[r"
        Welcome to this new release! Changes:

        ### Other

        - add config file
        - cargo init
        - Initial commit"]]
    .assert_eq(&gitea_release.body);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_does_not_releases_twice() {
    let context = TestContext::new().await;

    let crate_name = &context.gitea.repo;

    // Running `release` the first time, releases the project
    let outcome = context.run_release().success();
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "prs": [],
                "tag": "v0.1.0",
                "version": "0.1.0",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));

    // Running `release` the second time, releases nothing.
    let outcome = context.run_release().success();
    outcome.stdout("{\"releases\":[]}\n");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_can_do_backports() {
    let context = TestContext::new().await;

    let crate_name = &context.gitea.repo;

    // Running `release` the first time, releases the project
    let outcome = context.run_release().success();
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "prs": [],
                "tag": "v0.1.0",
                "version": "0.1.0",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));

    // Publish a new breaking release
    context.set_package_version(crate_name, &cargo_metadata::semver::Version::new(0, 2, 0));
    // We need to update the Cargo.lock file to reflect the new version
    context.run_cargo_check();
    context.push_all_changes("breaking release");
    let outcome = context.run_release().success();
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "prs": [],
                "tag": "v0.2.0",
                "version": "0.2.0",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));

    // Publish a backport release.
    // It's a backport because v0.1.1 < v0.2.0.
    context.set_package_version(crate_name, &cargo_metadata::semver::Version::new(0, 1, 1));
    // We need to update the Cargo.lock file to reflect the new version
    context.run_cargo_check();
    context.push_all_changes("backport release");
    let outcome = context.run_release().success();
    let expected_stdout = serde_json::json!({
        "releases": [
            {
                "package_name": crate_name,
                "prs": [],
                "tag": "v0.1.1",
                "version": "0.1.1",
            }
        ]
    })
    .to_string();
    outcome.stdout(format!("{expected_stdout}\n"));
}

/// How [`CARGO_WRAPPER`] and the registry behave during the release.
#[cfg(unix)]
#[derive(Debug)]
enum Scenario {
    /// `cargo publish` confirms that the package is in the registry.
    Confirmed,
    /// `cargo publish` doesn't print the `Published` status,
    /// like a Cargo whose wait for the package timed out.
    Unconfirmed,
    /// `cargo publish` confirms the package, but Cargo resolves dependencies from a
    /// mirror of the registry, which catches up only after the first failed check
    /// following publication.
    LaggingMirror,
    /// `cargo publish` confirms the package, and Cargo resolves dependencies from a
    /// local directory, which never receives the published package.
    VendoredSource,
}

/// Cargo wrapper that records the subcommands run by release-plz
/// and simulates the [`Scenario`] passed in `SCENARIO`.
#[cfg(unix)]
const CARGO_WRAPPER: &str = r#"#!/bin/sh
echo "$1" >> "$WRAPPER_DIR/calls"
if [ "$1" = info ] && [ "$SCENARIO" = LaggingMirror ] && grep -qx publish "$WRAPPER_DIR/calls" && [ ! -f "$WRAPPER_DIR/mirror_updated" ]; then
    "$REAL_CARGO" "$@"
    status=$?
    if [ "$status" = 0 ]; then
        echo 'mirror unexpectedly contains the new package' >&2
        exit 1
    fi
    git -C "$WRAPPER_DIR/mirror" pull --ff-only >&2 || exit 1
    touch "$WRAPPER_DIR/mirror_updated"
    exit "$status"
fi
if [ "$1" = publish ]; then
    "$REAL_CARGO" "$@" 2> "$WRAPPER_DIR/publish_stderr"
    status=$?
    if [ "$SCENARIO" = Unconfirmed ]; then
        grep -v Published "$WRAPPER_DIR/publish_stderr" >&2
    else
        cat "$WRAPPER_DIR/publish_stderr" >&2
    fi
    exit $status
fi
exec "$REAL_CARGO" "$@"
"#;

/// Release a new project through [`CARGO_WRAPPER`] in the given `scenario` and return
/// the `cargo info` and `cargo publish` subcommands run by release-plz, in order.
#[cfg(unix)]
async fn release_cargo_calls(scenario: Scenario) -> Vec<String> {
    let context = TestContext::new().await;
    let wrapper_dir = Utf8TempDir::new().unwrap();
    let cargo = wrapper_dir.path().join("cargo");
    fs_err::write(&cargo, CARGO_WRAPPER).unwrap();
    crate::helpers::make_executable(&cargo);

    let index = context.gitea.cargo_index_url();
    let replacement = match scenario {
        Scenario::Confirmed | Scenario::Unconfirmed => None,
        Scenario::LaggingMirror => {
            let mirror = wrapper_dir.path().join("mirror");
            assert_cmd::Command::new("git")
                .args(["clone", &index, mirror.as_str()])
                .assert()
                .success();
            let mirror_url = url::Url::from_directory_path(&mirror).unwrap();
            Some(format!("registry = {:?}", mirror_url.as_str()))
        }
        Scenario::VendoredSource => {
            let vendor = wrapper_dir.path().join("vendor");
            fs_err::create_dir(&vendor).unwrap();
            // If release-plz waited for the package to appear in the directory,
            // fail after a few seconds instead of the default 30 minutes.
            context.write_release_plz_toml("[workspace]\npublish_timeout = \"10s\"\n");
            Some(format!("directory = {:?}", vendor.as_str()))
        }
    };
    if let Some(replacement) = replacement {
        // The source name deliberately differs from the registry name: Cargo
        // matches replacements by index URL, not by registry name.
        context.append_cargo_config(&format!(
            "\n[source.origin]\nregistry = {index:?}\nreplace-with = \"replacement\"\n\
             [source.replacement]\n{replacement}\n"
        ));
        context.push_all_changes("replace registry source");
    }

    context
        .release_command()
        .env("CARGO", &cargo)
        .env("REAL_CARGO", env!("CARGO"))
        .env("WRAPPER_DIR", wrapper_dir.path())
        .env("SCENARIO", format!("{scenario:?}"))
        .assert()
        .success();

    if !matches!(scenario, Scenario::Unconfirmed) {
        let stderr = fs_err::read_to_string(wrapper_dir.path().join("publish_stderr")).unwrap();
        assert!(stderr.contains("Published"), "{stderr}");
    }
    let calls = fs_err::read_to_string(wrapper_dir.path().join("calls")).unwrap();
    calls
        .lines()
        .filter(|subcommand| ["info", "publish"].contains(subcommand))
        .map(str::to_string)
        .collect()
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_trusts_cargo_publish_confirmation() {
    // The first `cargo info` checks whether the package was already published.
    assert_eq!(
        release_cargo_calls(Scenario::Confirmed).await,
        ["info", "publish"]
    );
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_waits_for_package_if_cargo_publish_does_not_confirm() {
    assert_eq!(
        release_cargo_calls(Scenario::Unconfirmed).await,
        ["info", "publish", "info"]
    );
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_waits_for_mirror_after_cargo_publish_confirms() {
    assert_eq!(
        release_cargo_calls(Scenario::LaggingMirror).await,
        ["info", "publish", "info", "info"]
    );
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_trusts_cargo_publish_confirmation_with_vendored_sources() {
    // The directory never receives the package, so release-plz must not wait for it.
    assert_eq!(
        release_cargo_calls(Scenario::VendoredSource).await,
        ["info", "publish"]
    );
}
