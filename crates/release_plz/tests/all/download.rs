use cargo_utils::CARGO_TOML;
use release_plz_core::{PackageDownloader, fs_utils::Utf8TempDir};

use crate::helpers::test_context::TestContext;

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_preserves_request_order_and_skips_missing_packages() {
    let crates = ["first", "second", "third"];
    let context = TestContext::new_workspace(&crates).await;
    for name in crates {
        context.run_cargo_publish(name);
    }

    // Repeat in a fresh destination to cover both downloads and Cargo's local cache.
    for (requested, expected) in [
        (
            ["third", "missing", "first", "second"],
            ["third", "first", "second"],
        ),
        (
            ["second", "first", "missing", "third"],
            ["second", "first", "third"],
        ),
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
async fn download_rejects_duplicate_packages() {
    let context = TestContext::new().await;
    let name = &context.gitea.repo;
    context.run_cargo_publish(name);

    let dest_dir = Utf8TempDir::new().unwrap();
    let error = context
        .package_downloader([name, name], dest_dir.path())
        .download()
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("already exists and is not an empty directory"));
}

#[tokio::test]
async fn download_checks_destinations_before_downloading() {
    let crates = ["first", "occupied"];
    let registry_dir = Utf8TempDir::new().unwrap();
    let registry = git_cmd::Repo::init(registry_dir.path());
    let registry_url = url::Url::from_directory_path(registry_dir.path()).unwrap();
    // The index contains packages, but their archives are unavailable.
    let registry_config = serde_json::json!({ "dl": format!("{registry_url}missing") });
    fs_err::write(
        registry_dir.path().join("config.json"),
        registry_config.to_string(),
    )
    .unwrap();
    for name in crates {
        let index_dir = registry_dir.path().join(&name[..2]).join(&name[2..4]);
        fs_err::create_dir_all(&index_dir).unwrap();
        let entry = serde_json::json!({
            "name": name, "vers": "0.1.0", "deps": [],
            "cksum": "0".repeat(64), "features": {}, "yanked": false,
        });
        fs_err::write(index_dir.join(name), format!("{entry}\n")).unwrap();
    }
    registry.add_all_and_commit("add package index").unwrap();

    let cargo_dir = Utf8TempDir::new().unwrap();
    fs_err::create_dir(cargo_dir.path().join(".cargo")).unwrap();
    fs_err::write(
        cargo_dir.path().join(".cargo/config.toml"),
        format!("[registries.local]\nindex = '{registry_url}'\n"),
    )
    .unwrap();

    let dest_dir = Utf8TempDir::new().unwrap();
    let package_dir = dest_dir.path().join("occupied");
    fs_err::create_dir(&package_dir).unwrap();
    let marker = package_dir.join("marker");
    fs_err::write(&marker, "keep").unwrap();

    let downloader = PackageDownloader::new(crates, dest_dir.path().as_str())
        .with_registry("local".to_string())
        .with_cargo_cwd(cargo_dir.path().to_path_buf());
    let error = downloader.download().await.unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("already exists and is not an empty directory"),
        "{error}"
    );
    assert_eq!(fs_err::read_to_string(&marker).unwrap(), "keep");
    assert!(!package_dir.join(CARGO_TOML).exists());
    assert!(!dest_dir.path().join("first").exists());

    // An empty destination passes the check and reaches the failed download.
    fs_err::remove_file(marker).unwrap();
    let error = downloader.download().await.unwrap_err();
    assert!(format!("{error:#}").contains("failed to download packages"));
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_respects_source_replacement() {
    let context = TestContext::new().await;
    let name = &context.gitea.repo;
    context.run_cargo_publish(name);

    let config_path = context.repo_dir().join(".cargo/config.toml");
    let mut config = fs_err::read_to_string(&config_path).unwrap();
    let index = context.cargo_index_url();
    config.push_str(&format!(
        "\n[source.crates-io]\nreplace-with = 'local'\n[source.local]\nregistry = \"{index}\"\n"
    ));
    fs_err::write(config_path, config).unwrap();

    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = PackageDownloader::new([name], dest_dir.path().as_str())
        .with_cargo_cwd(context.repo_dir())
        .download()
        .await
        .unwrap();
    let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, [name]);
}
