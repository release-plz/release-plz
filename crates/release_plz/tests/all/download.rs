use release_plz_core::{PackageDownloader, fs_utils::Utf8TempDir};

use crate::helpers::{TEST_REGISTRY, test_context::TestContext};

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
}

#[tokio::test]
async fn download_returns_nothing_for_empty_requests() {
    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = PackageDownloader::new(Vec::<&str>::new(), dest_dir.path().as_str())
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
    let cargo_dir = Utf8TempDir::new().unwrap();
    // The local registry index contains packages, but their `.crate` archives are missing.
    for name in crates {
        let index_dir = cargo_dir
            .path()
            .join("registry/index")
            .join(&name[..2])
            .join(&name[2..4]);
        fs_err::create_dir_all(&index_dir).unwrap();
        let entry = serde_json::json!({
            "name": name, "vers": "0.1.0", "deps": [],
            "cksum": "0".repeat(64), "features": {}, "yanked": false,
        });
        fs_err::write(index_dir.join(name), format!("{entry}\n")).unwrap();
    }
    fs_err::create_dir(cargo_dir.path().join(".cargo")).unwrap();
    fs_err::write(
        cargo_dir.path().join(".cargo/config.toml"),
        "[source.crates-io]\nreplace-with = 'local'\n[source.local]\nlocal-registry = 'registry'\n",
    )
    .unwrap();

    let dest_dir = Utf8TempDir::new().unwrap();
    let package_dir = dest_dir.path().join("occupied");
    fs_err::create_dir(&package_dir).unwrap();
    let marker = package_dir.join("marker");
    fs_err::write(&marker, "keep").unwrap();

    let downloader = PackageDownloader::new(crates, dest_dir.path().as_str())
        .with_cargo_cwd(cargo_dir.path().to_path_buf());
    let error = downloader.download().await.unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("already exists and is not an empty directory"),
        "{error}"
    );
    assert_eq!(fs_err::read_to_string(&marker).unwrap(), "keep");
    assert!(!dest_dir.path().join("first").exists());

    // An empty destination passes the check and reaches the failed download.
    fs_err::remove_file(marker).unwrap();
    let error = downloader.download().await.unwrap_err();
    assert!(format!("{error:#}").contains("failed to download `"));
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_respects_source_replacement() {
    let context = TestContext::new().await;
    let name = &context.gitea.repo;
    context.run_cargo_publish(name);

    context.append_cargo_config(&format!(
        "\n[source.crates-io]\nreplace-with = '{TEST_REGISTRY}'\n"
    ));

    let dest_dir = Utf8TempDir::new().unwrap();
    let packages = PackageDownloader::new([name], dest_dir.path().as_str())
        .with_cargo_cwd(context.repo_dir())
        .download()
        .await
        .unwrap();
    let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, [name]);
}
