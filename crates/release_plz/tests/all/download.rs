use cargo_utils::CARGO_TOML;
use release_plz_core::{PackageDownloader, fs_utils::Utf8TempDir};

use crate::helpers::{TEST_REGISTRY, test_context::TestContext};

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn registry_batches_preserve_request_order_and_skip_missing_packages() {
    let context = TestContext::new_workspace(&["first", "second"]).await;
    context.run_cargo_publish("first");
    context.run_cargo_publish("second");

    // Repeat in a fresh destination to cover both downloads and Cargo's local cache.
    for requested in [
        ["second", "missing", "first"],
        ["first", "missing", "second"],
    ] {
        let dest_dir = Utf8TempDir::new().unwrap();
        let packages = PackageDownloader::new(requested, dest_dir.path().as_str())
            .with_registry(TEST_REGISTRY.to_string())
            .with_cargo_cwd(context.repo_dir())
            .download()
            .await
            .unwrap();

        let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, [requested[0], requested[2]]);
        for package in packages {
            let package_dir = dest_dir.path().join(package.name.as_str());
            assert_eq!(package.manifest_path, package_dir.join(CARGO_TOML));
            assert_eq!(
                fs_err::read_to_string(package_dir.join("src/main.rs")).unwrap(),
                fs_err::read_to_string(
                    context
                        .package_path(package.name.as_str())
                        .join("src/main.rs")
                )
                .unwrap()
            );
            assert!(!package_dir.join(".cargo-ok").exists());
        }
        assert_eq!(
            dest_dir.path().join("missing").read_dir().unwrap().count(),
            0
        );
    }

    for requested in [&["missing"][..], &[]] {
        let dest_dir = Utf8TempDir::new().unwrap();
        let packages = PackageDownloader::new(requested.iter().copied(), dest_dir.path().as_str())
            .with_registry(TEST_REGISTRY.to_string())
            .with_cargo_cwd(context.repo_dir())
            .download()
            .await
            .unwrap();
        assert!(packages.is_empty());
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn registry_batches_reject_duplicate_and_nonempty_destinations() {
    let context = TestContext::new().await;
    let name = &context.gitea.repo;
    context.run_cargo_publish(name);
    let dest_dir = Utf8TempDir::new().unwrap();
    let downloader = PackageDownloader::new([name, name], dest_dir.path().as_str())
        .with_registry(TEST_REGISTRY.to_string())
        .with_cargo_cwd(context.repo_dir());

    let error = downloader.download().await.unwrap_err();
    assert!(format!("{error:#}").contains("already exists and is not an empty directory"));
    let manifest = dest_dir.path().join(name).join(CARGO_TOML);
    let original_manifest = fs_err::read_to_string(&manifest).unwrap();

    // A subsequent batch must also leave the existing package intact.
    let error = downloader.download().await.unwrap_err();
    assert!(format!("{error:#}").contains("already exists and is not an empty directory"));
    assert_eq!(fs_err::read_to_string(manifest).unwrap(), original_manifest);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn registry_batches_respect_source_replacement() {
    let context = TestContext::new_workspace(&["first", "second"]).await;
    context.run_cargo_publish("first");
    context.run_cargo_publish("second");

    let config_path = context.repo_dir().join(".cargo/config.toml");
    let mut config = fs_err::read_to_string(&config_path).unwrap();
    let parsed: toml::Value = toml::from_str(&config).unwrap();
    let index = &parsed["registries"][TEST_REGISTRY]["index"];
    config.push_str(&format!(
        "\n[source.crates-io]\nreplace-with = 'local'\n[source.local]\nregistry = {index}\n"
    ));
    fs_err::write(config_path, config).unwrap();

    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = PackageDownloader::new(["second", "first"], dest_dir.path().as_str())
        .with_cargo_cwd(context.repo_dir())
        .download()
        .await
        .unwrap();
    let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["second", "first"]);
}
