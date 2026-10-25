use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::{
    TEST_REGISTRY,
    package::{PackageType, TestPackage},
    release_stdout,
    test_context::TestContext,
};

/// Neither package exists in the registry, including the optional dependency.
/// `dependency` is published to Cargo's default registry, while `dependent`
/// names the same registry in its `publish` field.
async fn unpublished_workspace() -> TestContext {
    let context = TestContext::new_workspace_with_packages(&[
        TestPackage::new("dependency").with_type(PackageType::Lib),
        TestPackage::new("dependent").with_path_dependencies(vec!["../dependency"]),
    ])
    .await;
    let root_manifest = context.repo_dir().join("Cargo.toml");
    let mut root = cargo_utils::LocalManifest::try_new(&root_manifest).unwrap();
    root.data["workspace"]["dependencies"]["dependency"] = toml_edit::value({
        let mut dependency = toml_edit::InlineTable::new();
        dependency.insert("path", "crates/dependency".into());
        dependency.insert("version", "0.1.0".into());
        dependency.insert("registry", TEST_REGISTRY.into());
        dependency
    });
    root.write().unwrap();
    let mut dependency =
        cargo_utils::LocalManifest::try_new(&context.package_path("dependency").join("Cargo.toml"))
            .unwrap();
    dependency.data["package"]
        .as_table_mut()
        .unwrap()
        .remove("publish");
    dependency.write().unwrap();
    let mut dependent =
        cargo_utils::LocalManifest::try_new(&context.package_path("dependent").join("Cargo.toml"))
            .unwrap();
    dependent.data["dependencies"]["dependency"] = toml_edit::value({
        let mut dependency = toml_edit::InlineTable::new();
        dependency.insert("workspace", true.into());
        dependency.insert("optional", true.into());
        dependency
    });
    dependent.write().unwrap();
    context.run_cargo_check();
    context.push_all_changes("use optional workspace dependency");
    context
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_checks_unpublished_workspace_dependencies() {
    let context = unpublished_workspace().await;
    for no_verify in [false, true] {
        // Exercise registry selection from package manifests as well as --registry.
        // Without --registry, the dry run only passes if release-plz recognizes that
        // the default registry of `dependency` is the one `dependent` names.
        let mut command = if no_verify {
            let mut command = context.release_command();
            command.arg("--no-verify");
            command
        } else {
            context.release_command_without_registry()
        };
        command
            .arg("--dry-run")
            .assert()
            .success()
            .stdout("{\"releases\":[]}\n");
    }
    for package in ["dependency", "dependent"] {
        assert!(
            context
                .cargo_target_dir()
                .join(format!("package/{package}-0.1.0/Cargo.toml"))
                .exists()
        );
    }
    let downloaded = Utf8TempDir::new().unwrap();
    let packages = context
        .package_downloader(["dependency", "dependent"], downloaded.path())
        .download()
        .await
        .unwrap();
    assert!(packages.is_empty(), "a dry run must not upload packages");
    assert!(context.repo.git(&["tag", "--list"]).unwrap().is_empty());
    context.repo.is_clean().unwrap();

    fs_err::write(
        context.package_path("dependent").join("src/main.rs"),
        "compile_error!(\"verification must still run\");",
    )
    .unwrap();
    context.push_all_changes("break dependent compilation");
    let output = context
        .release_command()
        .arg("--dry-run")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(stderr.contains("verification must still run"), "{stderr}");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_preserves_package_selection() {
    let context = TestContext::new_workspace(&[
        "selected",
        "selected-too",
        "published",
        "tagged",
        "disabled",
        "unpublished",
        "git-only",
    ])
    .await;
    context.run_cargo_publish("published");
    context.repo.tag_lightweight("tagged-v0.1.0").unwrap();
    // Including any skipped package in Cargo's batch would fail verification.
    for name in ["published", "tagged", "disabled", "unpublished", "git-only"] {
        fs_err::write(
            context.package_path(name).join("src/main.rs"),
            "compile_error!(\"this package must not be verified\");",
        )
        .unwrap();
    }
    context.write_release_plz_toml(
        r#"
[[package]]
name = "disabled"
release = false
[[package]]
name = "unpublished"
publish = false
[[package]]
name = "git-only"
git_only = true
"#,
    );
    context
        .release_command()
        .arg("--dry-run")
        .assert()
        .success()
        .stdout("{\"releases\":[]}\n");
    for name in ["selected", "selected-too"] {
        assert!(
            context
                .cargo_target_dir()
                .join(format!("package/{name}-0.1.0/Cargo.toml"))
                .exists()
        );
    }
    assert_eq!(
        context.repo.git(&["tag", "--list"]).unwrap(),
        "tagged-v0.1.0"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_preserves_per_package_options() {
    let context = TestContext::new_workspace(&["one", "two", "unchecked"]).await;
    for (name, feature) in [("one", "only_one"), ("two", "only_two")] {
        let manifest_path = context.package_path(name).join("Cargo.toml");
        let mut manifest = cargo_utils::LocalManifest::try_new(&manifest_path).unwrap();
        manifest.data["features"][feature] = toml_edit::value(toml_edit::Array::new());
        manifest.write().unwrap();
        fs_err::write(
            context.package_path(name).join("src/main.rs"),
            format!(
                "#[cfg(not(feature = {feature:?}))]\ncompile_error!(\"missing feature\");\nfn main() {{}}"
            ),
        )
        .unwrap();
    }
    context.write_release_plz_toml(
        r#"
[[package]]
name = "one"
publish_features = ["only_one"]
[[package]]
name = "two"
publish_all_features = true
[[package]]
name = "unchecked"
publish_no_verify = true
publish_allow_dirty = true
"#,
    );
    fs_err::write(
        context.package_path("unchecked").join("src/main.rs"),
        "compile_error!(\"do not compile this dirty package\");",
    )
    .unwrap();
    context
        .release_command()
        .arg("--dry-run")
        .assert()
        .success();
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_checks_packages_with_features_individually() {
    let context = TestContext::new_workspace(&["featured", "plain"]).await;
    let manifest_path = context.package_path("featured").join("Cargo.toml");
    let mut manifest = cargo_utils::LocalManifest::try_new(&manifest_path).unwrap();
    manifest.data["features"]["extra"] = toml_edit::value(toml_edit::Array::new());
    manifest.write().unwrap();
    context.write_release_plz_toml("[workspace]\npublish_features = [\"extra\"]\n");
    // The real release fails on `cargo publish -p plain --features extra` because
    // `plain` doesn't define `extra`. Checking both packages together would pass,
    // because Cargo only requires one of the selected packages to define the feature.
    let output = context
        .release_command()
        .arg("--dry-run")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("failed to dry-run publishing plain:"),
        "{stderr}"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_preserves_per_package_registries() {
    let context = TestContext::new_workspace(&["one", "two"]).await;
    let other_registry = TestContext::new().await;
    context.append_cargo_config(&format!(
        "\n[registries.other]\nindex = {:?}\n",
        other_registry.gitea.cargo_index_url()
    ));
    let manifest_path = context.package_path("two").join("Cargo.toml");
    let mut manifest = cargo_utils::LocalManifest::try_new(&manifest_path).unwrap();
    let mut registries = toml_edit::Array::new();
    registries.push("other");
    manifest.data["package"]["publish"] = toml_edit::value(registries);
    manifest.write().unwrap();
    context.push_all_changes("use different registries");
    context
        .release_command_without_registry()
        .arg("--dry-run")
        .assert()
        .success();

    // The real release used to run `cargo publish -p two` without `--registry`, so
    // Cargo picked the default registry, which isn't in the `publish` field of `two`.
    context
        .release_command_without_registry()
        .env(
            cargo_utils::cargo_registries_token_env_var_name("other").unwrap(),
            format!("Bearer {}", other_registry.gitea.token),
        )
        .assert()
        .success();
    for (package, registry) in [("one", TEST_REGISTRY), ("two", "other")] {
        let downloaded = Utf8TempDir::new().unwrap();
        let packages = context
            .package_downloader([package], downloaded.path())
            .with_registry(registry.to_string())
            .download()
            .await
            .unwrap();
        assert_eq!(packages.len(), 1, "{package} isn't in {registry}");
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_explains_incompatible_dependency_options() {
    let context = unpublished_workspace().await;
    context.write_release_plz_toml(
        r#"
[[package]]
name = "dependency"
publish_no_verify = true
"#,
    );
    let output = context
        .release_command()
        .arg("--dry-run")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("failed to dry-run publishing dependent"),
        "{stderr}"
    );
    assert!(
        stderr.contains("If a package depends on an unpublished workspace package"),
        "{stderr}"
    );
    assert!(stderr.contains("publish_no_verify"), "{stderr}");
}

#[cfg(unix)]
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_dry_run_checks_packages_individually_with_old_cargo() {
    let context = TestContext::new_workspace(&["one", "two"]).await;
    // Cargo 1.89 can't publish multiple packages with a single command,
    // so fail if release-plz checks the packages together.
    let cargo_dir = fake_cargo(
        r#"#!/bin/sh
if [ "$1" = --version ]; then
    echo 'cargo 1.89.0 (c24e10642 2025-06-23)'
    exit 0
fi
if [ "$1" = publish ] && [ "$(printf '%s\n' "$@" | grep -cx -- --package)" -gt 1 ]; then
    echo 'error: Cargo 1.89 publishes one package at a time' >&2
    exit 1
fi
exec "$REAL_CARGO" "$@"
"#,
    );
    let output = context
        .release_command()
        .arg("--dry-run")
        .env("CARGO", cargo_dir.path().join("cargo"))
        .env("REAL_CARGO", env!("CARGO"))
        .env("RELEASE_PLZ_LOG", "WARN")
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(stderr.contains("requires Cargo 1.90 or newer"), "{stderr}");
    // The packages are independent, so release-plz can check them individually.
    for package in ["one", "two"] {
        assert!(
            context
                .cargo_target_dir()
                .join(format!("package/{package}-0.1.0/Cargo.toml"))
                .exists()
        );
    }
}

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
    outcome.stdout(release_stdout(crate_name, &expected_tag, "0.1.0"));

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
    outcome.stdout(release_stdout(crate_name, "v0.1.0", "0.1.0"));

    // Running `release` the second time, releases nothing.
    let outcome = context.run_release().success();
    outcome.stdout("{\"releases\":[]}\n");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_publishes_with_the_token_of_the_package_registry() {
    let context = TestContext::new().await;

    let crate_name = &context.gitea.repo;

    // Without `--registry`, the package is published to the test registry because it's
    // the one in its `publish` field. The crates.io token is invalid for the test registry,
    // so `cargo publish` only succeeds if release-plz uses the token of the test registry.
    let outcome = context
        .release_command_without_registry()
        .env("CARGO_REGISTRY_TOKEN", "crates-io-token")
        .assert()
        .success();
    outcome.stdout(release_stdout(crate_name, "v0.1.0", "0.1.0"));
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_can_do_backports() {
    let context = TestContext::new().await;

    let crate_name = &context.gitea.repo;

    // Running `release` the first time, releases the project
    let outcome = context.run_release().success();
    outcome.stdout(release_stdout(crate_name, "v0.1.0", "0.1.0"));

    // Publish a new breaking release
    context.set_package_version(crate_name, &cargo_metadata::semver::Version::new(0, 2, 0));
    // We need to update the Cargo.lock file to reflect the new version
    context.run_cargo_check();
    context.push_all_changes("breaking release");
    let outcome = context.run_release().success();
    outcome.stdout(release_stdout(crate_name, "v0.2.0", "0.2.0"));

    // Publish a backport release.
    // It's a backport because v0.1.1 < v0.2.0.
    context.set_package_version(crate_name, &cargo_metadata::semver::Version::new(0, 1, 1));
    // We need to update the Cargo.lock file to reflect the new version
    context.run_cargo_check();
    context.push_all_changes("backport release");
    let outcome = context.run_release().success();
    outcome.stdout(release_stdout(crate_name, "v0.1.1", "0.1.1"));
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

/// Create a directory with a `cargo` executable that runs `script`.
/// The script can run the real Cargo with `$REAL_CARGO`.
#[cfg(unix)]
fn fake_cargo(script: &str) -> Utf8TempDir {
    let dir = Utf8TempDir::new().unwrap();
    let cargo = dir.path().join("cargo");
    fs_err::write(&cargo, script).unwrap();
    crate::helpers::make_executable(&cargo);
    dir
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
    let wrapper_dir = fake_cargo(CARGO_WRAPPER);

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
        .env("CARGO", wrapper_dir.path().join("cargo"))
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
