use cargo_utils::LocalManifest;

use crate::helpers::test_context::TestContext;

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_should_set_custom_pr_details() {
    let context = TestContext::new().await;

    let config = r#"
    [workspace]
    pr_name = "release: {{ package }} {{ version }}" 
    "#;

    context.write_release_plz_toml(config);
    context.run_release_pr().success();

    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    assert_eq!(
        opened_prs[0].title,
        format!("release: {} 0.1.0", context.gitea.repo)
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_should_not_provide_details_for_multi_package_pr() {
    let context = TestContext::new_workspace(&["crates/one", "crates/two"]).await;

    let config = r#"
    [workspace]
    pr_name = "release: {{ package }} {{ version }}" 
    "#;

    context.write_release_plz_toml(config);
    context.run_release_pr().success();

    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    // 'package' is empty when releasing multiple packages
    assert_eq!(opened_prs[0].title, format!("release:  0.1.0"));

    // Make one of the packages have a different version
    change_version(&context, "crates/one", "0.2.0");
    context.run_release_pr().success();

    let opened_prs = context.opened_release_prs().await;
    assert_eq!(opened_prs.len(), 1);
    // 'version' is empty when multiple packages with different versions are released
    assert_eq!(opened_prs[0].title, format!("release:  "));
}

#[tokio::test]
#[ignore = "This test fails in CI, but works locally on MacOS. TODO: fix this."]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_detects_edited_readme_cargo_toml_field() {
    let context = TestContext::new().await;

    context.run_release_pr().success();
    context.merge_release_pr().await;

    let expected_tag = "v0.1.0";

    context.run_release().success();

    let gitea_release = context.gitea.get_gitea_release(expected_tag).await;
    assert_eq!(gitea_release.name, expected_tag);

    move_readme(&context, "move readme");

    context.run_release_pr().success();
    context.merge_release_pr().await;

    let expected_tag = "v0.1.1";

    context.run_release().success();

    let gitea_release = context.gitea.get_gitea_release(expected_tag).await;
    assert_eq!(gitea_release.name, expected_tag);
    expect_test::expect![[r#"
        ### Other

        - move readme"#]]
    .assert_eq(&gitea_release.body);
}

#[tokio::test]
#[ignore = "This test fails in CI, but works locally on MacOS. TODO: fix this."]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn release_plz_honors_features_always_increment_minor_flag() {
    let context = TestContext::new().await;

    let config = r#"
    [workspace]
    features_always_increment_minor = true
    "#;
    context.write_release_plz_toml(config);

    context.run_release_pr().success();
    context.merge_release_pr().await;

    let expected_tag = "v0.1.0";

    context.run_release().success();

    let gitea_release = context.gitea.get_gitea_release(expected_tag).await;
    assert_eq!(gitea_release.name, expected_tag);

    move_readme(&context, "feat: move readme");

    context.run_release_pr().success();
    context.merge_release_pr().await;

    let expected_tag = "v0.2.0";

    context.run_release().success();

    let gitea_release = context.gitea.get_gitea_release(expected_tag).await;
    assert_eq!(gitea_release.name, expected_tag);
    expect_test::expect![[r#"
        ### Added

        - move readme"#]]
    .assert_eq(&gitea_release.body);
}

fn change_version(context: &TestContext, package: &str, version: &str) {
    let cargo_toml_path = context.repo_dir().join(package).join("Cargo.toml");
    let mut cargo_toml = LocalManifest::try_new(&cargo_toml_path).unwrap();
    cargo_toml.data["package"]["version"] = toml_edit::value(version);
    cargo_toml.write().unwrap();

    context.repo.add_all_and_commit("change version").unwrap();
    context.repo.git(&["push"]).unwrap();
}

fn move_readme(context: &TestContext, message: &str) {
    let readme = "README.md";
    let new_readme = format!("NEW_{readme}");
    let old_readme_path = context.repo_dir().join(readme);
    let new_readme_path = context.repo_dir().join(&new_readme);
    fs_err::rename(old_readme_path, new_readme_path).unwrap();

    let cargo_toml_path = context.repo_dir().join("Cargo.toml");
    let mut cargo_toml = LocalManifest::try_new(&cargo_toml_path).unwrap();
    cargo_toml.data["package"]["readme"] = toml_edit::value(new_readme);
    cargo_toml.write().unwrap();

    context.repo.add_all_and_commit(message).unwrap();
    context.repo.git(&["push"]).unwrap();
}
