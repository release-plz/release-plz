use cargo_utils::CARGO_TOML;
use release_plz_core::{PackageDownloader, fs_utils::Utf8TempDir};

use crate::helpers::test_context::TestContext;

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_preserves_request_order_and_skips_missing_packages() {
    let context = TestContext::new_workspace(&["first", "second"]).await;
    context.run_cargo_publish("first");
    context.run_cargo_publish("second");

    // Repeat in a fresh destination to cover both downloads and Cargo's local cache.
    for (requested, expected) in [
        (["second", "missing", "first"], ["second", "first"]),
        (["first", "missing", "second"], ["first", "second"]),
    ] {
        let dest_dir = Utf8TempDir::new().unwrap();
        let packages = context
            .package_downloader(requested, dest_dir.path())
            .download()
            .await
            .unwrap();

        let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, expected);
        for package in packages {
            let package_dir = dest_dir.path().join(package.name.as_str());
            assert!(!package_dir.join(".cargo-ok").exists());
        }
        assert!(!dest_dir.path().join("missing").exists());
    }

    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = context
        .package_downloader(Vec::<&str>::new(), dest_dir.path())
        .download()
        .await
        .unwrap();
    assert!(packages.is_empty());
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_rejects_duplicate_and_nonempty_destinations() {
    let context = TestContext::new().await;
    let name = &context.gitea.repo;
    context.run_cargo_publish(name);
    let nonempty_error = "already exists and is not an empty directory";

    let dest_dir = Utf8TempDir::new().unwrap();
    let error = context
        .package_downloader([name, name], dest_dir.path())
        .download()
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains(nonempty_error));

    let dest_dir = Utf8TempDir::new().unwrap();
    let package_dir = dest_dir.path().join(name);
    let marker = package_dir.join("marker");
    fs_err::create_dir(&package_dir).unwrap();
    fs_err::write(&marker, "keep").unwrap();
    let error = context
        .package_downloader([name], dest_dir.path())
        .download()
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains(nonempty_error));
    assert_eq!(fs_err::read_to_string(marker).unwrap(), "keep");
    assert!(!package_dir.join(CARGO_TOML).exists());
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_respects_source_replacement() {
    let context = TestContext::new_workspace(&["first", "second"]).await;
    context.run_cargo_publish("first");
    context.run_cargo_publish("second");

    let config_path = context.repo_dir().join(".cargo/config.toml");
    let mut config = fs_err::read_to_string(&config_path).unwrap();
    let index = context.cargo_index_url();
    config.push_str(&format!(
        "\n[source.crates-io]\nreplace-with = 'local'\n[source.local]\nregistry = \"{index}\"\n"
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
