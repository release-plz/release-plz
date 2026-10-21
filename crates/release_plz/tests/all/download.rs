use release_plz_core::{PackageDownloader, fs_utils::Utf8TempDir};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

use crate::helpers::{TEST_REGISTRY, TEST_SPARSE_REGISTRY, test_context::TestContext};

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn download_preserves_request_order_and_skips_missing_packages() {
    let crates = ["first", "second", "third"];
    let context = TestContext::new_workspace(&crates).await;
    for name in crates {
        context.run_cargo_publish(name);
    }

    // Over the sparse protocol every index file is a separate HTTP request,
    // so the queries run concurrently and can complete in any order.
    for registry in [TEST_REGISTRY, TEST_SPARSE_REGISTRY] {
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
                .with_registry(registry.to_string())
                .download()
                .await
                .unwrap();

            let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, expected, "registry: {registry}");
            for package in packages {
                let package_dir = dest_dir.path().join(package.name.as_str());
                assert!(!package_dir.join(".cargo-ok").exists());
            }
            assert!(!dest_dir.path().join("missing").exists());
        }
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

    // Over the sparse protocol, the second query for the same name waits for the
    // first one's in-flight index fetch: a path the git registry never takes.
    for registry in [TEST_REGISTRY, TEST_SPARSE_REGISTRY] {
        let dest_dir = Utf8TempDir::new().unwrap();
        let error = context
            .package_downloader([name, name], dest_dir.path())
            .with_registry(registry.to_string())
            .download()
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("already exists and is not an empty directory"),
            "registry: {registry}"
        );
    }
}

#[tokio::test]
async fn download_checks_destinations_before_downloading() {
    let crates = ["first", "occupied"];
    let cargo_dir = cargo_dir_with_config(
        "[source.crates-io]\nreplace-with = 'local'\n[source.local]\nlocal-registry = 'registry'\n",
    );
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
async fn download_stops_querying_after_failed_registry_config() {
    let server = MockServer::start().await;
    // Cargo doesn't keep a failed `config.json` in memory, and there's no copy in the
    // index cache, which is keyed by registry URL: only this test uses this URL path,
    // and it never gets a `config.json`. So each query still running after the first
    // failure would fetch `config.json` again, one after another.
    Mock::given(path("/config-failure/config.json"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    // Without retries, every attempt to load `config.json` is a single request.
    let cargo_dir = cargo_dir_with_config(&format!(
        "[registries.mock]\nindex = 'sparse+{}/config-failure/'\n[net]\nretry = 0\n",
        server.uri()
    ));

    let dest_dir = Utf8TempDir::new().unwrap();
    let error = PackageDownloader::new(["first", "second", "third"], dest_dir.path().as_str())
        .with_registry("mock".to_string())
        .with_cargo_cwd(cargo_dir.path().to_path_buf())
        .download()
        .await
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("failed to query package first"),
        "{error:#}"
    );
    server.verify().await;
}

#[tokio::test]
async fn download_returns_query_error_before_other_index_requests_finish() {
    use std::time::Duration;
    use tokio::{
        io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
        net::{TcpListener, TcpStream},
        sync::oneshot,
        time::timeout,
    };

    async fn read_request(stream: &mut TcpStream) -> String {
        let mut lines = BufReader::new(stream).lines();
        let request = lines.next_line().await.unwrap().unwrap();
        while !lines.next_line().await.unwrap().unwrap().is_empty() {}
        request
    }

    async fn respond(mut stream: TcpStream, status: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release_pending, pending_released) = oneshot::channel();
    let (queries_started, queries_ready) = oneshot::channel();
    let server = tokio::spawn(async move {
        // Cargo fetches config.json before starting the package queries.
        let (mut config, _) = listener.accept().await.unwrap();
        let request = read_request(&mut config).await;
        assert!(request.contains("/config.json "));
        respond(config, "200 OK", r#"{"dl":"http://127.0.0.1/unused"}"#).await;

        // Wait for both queries before failing the first. Keep the second response
        // open until the caller has received the error; no response-delay timing is needed.
        let mut requests = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            requests.push((request, stream));
        }
        requests.sort_by(|(a, _), (b, _)| a.cmp(b));
        let (second_request, second) = requests.pop().unwrap();
        let (first_request, first) = requests.pop().unwrap();
        assert!(first_request.contains("/fi/rs/first "));
        assert!(second_request.contains("/se/co/second "));
        queries_started.send(()).unwrap();
        respond(first, "503 Service Unavailable", "").await;
        pending_released.await.unwrap();
        respond(second, "404 Not Found", "").await;
    });

    // Disable HTTP/2 pipe waiting so both HTTP/1.1 requests can start before either responds.
    let cargo_dir = cargo_dir_with_config(&format!(
        "[registries.mock]\nindex = 'sparse+http://{address}/'\n[net]\nretry = 0\n[http]\nmultiplexing = false\n"
    ));
    let dest_dir = Utf8TempDir::new().unwrap();
    let downloader = PackageDownloader::new(["first", "second"], dest_dir.path().as_str())
        .with_registry("mock".to_string())
        .with_cargo_cwd(cargo_dir.path().to_path_buf());
    // If the downloader regresses to waiting for Cargo's HTTP cleanup, `download()` blocks
    // until the pending response is released. Run it on a blocking thread so this task can
    // still release the response and fail the test instead of hanging.
    let mut download = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(downloader.download())
    });
    // Other tests can hold Cargo's package-cache lock. Only time the error return
    // after our queries have acquired it and reached the server.
    tokio::select! {
        // The server fails the first query right after reporting both, so the download
        // may have finished too.
        biased;
        ready = queries_ready => ready.unwrap(),
        // E.g. the queries run one at a time, or the registry can't be reached.
        result = &mut download => {
            panic!("download finished before both queries reached the server: {result:?}")
        }
    }
    let result = timeout(Duration::from_secs(10), download).await;
    release_pending.send(()).unwrap();
    server.await.unwrap();
    let error = result
        .expect("query errors must return before unrelated index requests finish")
        .unwrap()
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("failed to query package first"),
        "{error:#}"
    );
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

/// Create a directory where Cargo runs with `config` as its configuration.
fn cargo_dir_with_config(config: &str) -> Utf8TempDir {
    let dir = Utf8TempDir::new().unwrap();
    fs_err::create_dir(dir.path().join(".cargo")).unwrap();
    fs_err::write(dir.path().join(".cargo/config.toml"), config).unwrap();
    dir
}
