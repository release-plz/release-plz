use super::*;
use crate::{GitForge, GitHub, GitTagConfig, ReleaseConfig};
use cargo_metadata::{Metadata, camino::Utf8Path};
use secrecy::SecretString;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_bytes, body_json, header, method, path, query_param},
};

fn package() -> Package {
    let metadata = fake_package::metadata::fake_metadata();
    metadata
        .packages
        .into_iter()
        .find(|p| metadata.workspace_members.contains(&p.id))
        .unwrap()
}

/// A receipt for matrix slot `index`: one executable archive plus its checksum.
fn receipt(index: usize, total: usize) -> Receipt {
    let package = package();
    serde_json::from_value(json!({
        "schema": 1, "tag": "v1.0.0", "commit": "abc",
        "job": {"run_id": "123", "index": index, "total": total, "target": format!("target-{index}")},
        "assets": [
            {"id": index * 2 + 1, "name": format!("app-{index}.zip"), "size": 100, "state": "uploaded"},
            {"id": index * 2 + 2, "name": format!("app-{index}.sha256"), "size": 64, "state": "uploaded"},
        ],
        "manifest": {
            "dist_version": cargo_dist::VERSION, "announcement_tag": "v1.0.0",
            "releases": [{"app_name": package.name, "app_version": package.version.to_string()}],
            "artifacts": {
                format!("app-{index}.zip"): {
                    "name": format!("app-{index}.zip"), "kind": "executable-zip", "target_triples": [format!("target-{index}")]
                },
                format!("app-{index}.sha256"): {
                    "name": format!("app-{index}.sha256"), "kind": "checksum", "target_triples": []
                },
            }
        }
    })).unwrap()
}

/// The release assets as they look when every receipt's upload is still in place.
fn assets(receipts: &[Receipt]) -> Vec<Asset> {
    receipts
        .iter()
        .flat_map(|r| r.assets.iter().cloned())
        .collect()
}

fn validate(receipts: &[Receipt], assets: &[Asset]) -> anyhow::Result<Vec<String>> {
    validate_receipts(receipts, "123", "v1.0.0", "abc", &package(), assets)
}

#[track_caller]
fn assert_rejected(receipts: &[Receipt], assets: &[Asset], message: &str) {
    let error = validate(receipts, assets).unwrap_err().to_string();
    assert!(error.contains(message), "{error}");
}

#[test]
fn only_complete_matching_matrix_can_be_finalized() {
    let mut receipts = vec![receipt(0, 2)];
    assert_rejected(
        &receipts,
        &assets(&receipts),
        "distribution matrix is incomplete: got 1 of 2 builds",
    );
    receipts.push(receipt(1, 2));
    assert_eq!(
        validate(&receipts, &assets(&receipts)).unwrap(),
        ["target-0", "target-1"]
    );
    for (run, tag, commit) in [
        ("124", "v1.0.0", "abc"),
        ("123", "v2.0.0", "abc"),
        ("123", "v1.0.0", "def"),
    ] {
        let error = validate_receipts(&receipts, run, tag, commit, &package(), &assets(&receipts))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("does not match this run, tag or commit"),
            "{error}"
        );
    }
    let mut replaced = assets(&receipts);
    replaced[0].id += 100;
    assert_rejected(
        &receipts,
        &replaced,
        "release asset `app-0.zip` is missing or was replaced",
    );
    receipts[1].job.target = receipts[0].job.target.clone();
    assert_rejected(
        &receipts,
        &assets(&receipts),
        "multiple matrix jobs built the same target",
    );
}

#[test]
fn receipts_cannot_omit_artifacts_or_reuse_matrix_slots() {
    let mut receipts = vec![receipt(0, 2), receipt(1, 2)];
    receipts[1].job.index = 0;
    assert_rejected(
        &receipts,
        &assets(&receipts),
        "inconsistent matrix receipts",
    );
    receipts[1].job.index = 1;
    validate(&receipts, &assets(&receipts)).unwrap();
    // Dropping the checksum keeps the binary but leaves a manifest artifact unaccounted for.
    receipts[1].assets.pop();
    assert_rejected(
        &receipts,
        &assets(&receipts),
        "receipt is missing artifact `app-1.sha256`",
    );
    receipts[1].assets.clear();
    assert_rejected(
        &receipts,
        &assets(&receipts),
        "distribution receipt contains no binaries",
    );
    let mut receipts = vec![receipt(0, 1)];
    receipts[0]
        .manifest
        .artifacts
        .get_mut("app-0.zip")
        .unwrap()
        .path = Some("app-0.zip".into());
    assert_rejected(
        &receipts,
        &assets(&receipts),
        "distribution receipt contains local paths",
    );
}

#[test]
fn receipts_require_uploaded_assets_of_the_recorded_size() {
    let receipts = vec![receipt(0, 1)];
    // GitHub reports an interrupted upload as a `starter` asset.
    let mut pending = assets(&receipts);
    pending[0].state = "starter".into();
    assert_rejected(
        &receipts,
        &pending,
        "release asset `app-0.zip` is missing or was replaced",
    );
    let mut truncated = assets(&receipts);
    truncated[1].size -= 1;
    assert_rejected(
        &receipts,
        &truncated,
        "release asset `app-0.sha256` is missing or was replaced",
    );
}

#[test]
fn manifest_must_describe_exactly_this_release() {
    let package = package();
    receipt(0, 1).manifest.validate("v1.0.0", &package).unwrap();
    const WRONG_RELEASE: &str = "cargo-dist must select exactly the requested package and version";
    type Mutation = fn(&mut Manifest);
    let cases: [(Mutation, &str); 5] = [
        (
            |manifest| manifest.dist_version = "0.0.1".into(),
            "unexpected cargo-dist version",
        ),
        (
            |manifest| manifest.announcement_tag = "v2.0.0".into(),
            "cargo-dist tag does not match release",
        ),
        (
            |manifest| {
                manifest.releases.push(ManifestRelease {
                    app_name: "other".into(),
                    app_version: "1.0.0".into(),
                    extra: BTreeMap::new(),
                });
            },
            WRONG_RELEASE,
        ),
        (
            |manifest| manifest.releases[0].app_name = "other".into(),
            WRONG_RELEASE,
        ),
        (
            |manifest| manifest.releases[0].app_version = "9.9.9".into(),
            WRONG_RELEASE,
        ),
    ];
    for (mutate, message) in cases {
        let mut manifest = receipt(0, 1).manifest;
        mutate(&mut manifest);
        let error = manifest
            .validate("v1.0.0", &package)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn release_notes_preserve_existing_changelog() {
    let body =
        body_with_installation_notes("## Fixes\n\nFixed things.\n", "## Downloads\n\nA link.\n");
    assert!(body.starts_with("## Fixes\n\nFixed things.\n\n"));
    assert!(body.contains("## Downloads\n\nA link."));
    let mut manifest = receipt(0, 1).manifest;
    manifest.announcement_github_body =
        Some("## Release Notes\n\nDuplicate changelog.\n\n## Download app\n".into());
    manifest.announcement_changelog = Some("Duplicate changelog.".into());
    assert_eq!(manifest.installation_notes().unwrap(), "## Download app\n");
}

#[test]
fn release_notes_are_replaced_when_finalize_reruns() {
    let changelog = "## Fixes\n\nFixed things.\n";
    let once = body_with_installation_notes(changelog, "## Downloads\n\nA link.\n");
    assert_eq!(
        body_with_installation_notes(&once, "## Downloads\n\nA link.\n"),
        once
    );
    let updated = body_with_installation_notes(&once, "## Downloads\n\nAnother link.\n");
    assert!(updated.starts_with("## Fixes\n\nFixed things.\n\n<!-- release-plz-dist -->\n"));
    assert!(updated.contains("Another link."));
    assert!(!updated.contains("A link."));
    assert_eq!(updated.matches("<!-- release-plz-dist -->").count(), 1);
    assert_eq!(updated.matches("<!-- /release-plz-dist -->").count(), 1);
}

#[test]
fn release_notes_keep_text_added_after_the_installation_notes() {
    // A draft kept by `git_release_draft` can be edited before finalize runs again.
    let notes = "## Downloads\n\nA link.\n";
    let once = body_with_installation_notes("## Fixes\n\nFixed things.\n", notes);
    let edited = format!("{once}\n## Known issues\n\nNone yet.\n");
    assert_eq!(body_with_installation_notes(&edited, notes), edited);
    let updated = body_with_installation_notes(&edited, "## Downloads\n\nAnother link.\n");
    assert!(updated.starts_with("## Fixes\n\nFixed things.\n\n<!-- release-plz-dist -->\n"));
    assert!(updated.ends_with("<!-- /release-plz-dist -->\n\n## Known issues\n\nNone yet.\n"));
    assert!(updated.contains("Another link."));
    assert!(!updated.contains("A link."));
}

#[test]
fn release_notes_without_a_changelog_start_with_the_marker() {
    let notes = "## Downloads\n\nA link.\n";
    let body = body_with_installation_notes("", notes);
    assert!(body.starts_with("<!-- release-plz-dist -->\n## Downloads"));
    assert!(body.ends_with("A link.\n<!-- /release-plz-dist -->\n"));
    assert_eq!(body_with_installation_notes(&body, notes), body);
}

#[test]
fn matrix_identity_defaults_to_a_single_host_job() {
    let job = DistJob::from_matrix("123".into(), "", "", "null").unwrap();
    assert_eq!((job.index, job.total), (0, 1));
    assert_eq!(job.target, cargo_dist::host_target().unwrap());
    let job = DistJob::from_matrix(
        "123".into(),
        "1",
        "2",
        r#"{"target": "aarch64-apple-darwin"}"#,
    )
    .unwrap();
    assert_eq!((job.index, job.total), (1, 2));
    assert_eq!(job.target, "aarch64-apple-darwin");
    let job = DistJob::from_matrix("123".into(), "0", "1", r#"{"target": null}"#).unwrap();
    assert_eq!(job.target, cargo_dist::host_target().unwrap());
}

#[test]
fn matrix_identity_rejects_inconsistent_or_invalid_slots() {
    let target = r#"{"target": "x86_64-unknown-linux-gnu"}"#;
    for (run_id, index, total, matrix, message) in [
        (
            "123",
            "0",
            "",
            "null",
            "both matrix job index and total must be provided",
        ),
        (
            "123",
            "",
            "2",
            "null",
            "both matrix job index and total must be provided",
        ),
        (
            "123",
            "",
            "",
            target,
            "matrix context requires a job index and total",
        ),
        (
            "123",
            "0",
            "1",
            r#"{"target": 1}"#,
            "matrix.target must be a Rust target triple string",
        ),
        ("123", "x", "1", target, "invalid matrix job number `x`"),
        (
            "123",
            "1",
            "1",
            target,
            "invalid distribution matrix identity",
        ),
        (
            "123",
            "0",
            "257",
            target,
            "invalid distribution matrix identity",
        ),
        (
            "123",
            "0",
            "0",
            target,
            "invalid distribution matrix identity",
        ),
        ("abc", "0", "1", target, "invalid GitHub run ID"),
        // Targets become file names and command-line arguments.
        (
            "123",
            "0",
            "1",
            r#"{"target": "x86_64/evil"}"#,
            "invalid Rust target triple",
        ),
        (
            "123",
            "0",
            "1",
            r#"{"target": ""}"#,
            "invalid Rust target triple",
        ),
        ("123", "0", "1", "{", "invalid distribution matrix context"),
    ] {
        let error = DistJob::from_matrix(run_id.into(), index, total, matrix)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{error}");
    }
}

fn client(server: &MockServer) -> GitClient {
    GitClient::new(GitForge::Github(
        GitHub::new("owner".into(), "repo".into(), SecretString::from("token"))
            .with_base_url(server.uri().parse().unwrap()),
    ))
    .unwrap()
}

/// Release `1` for `v1.0.0`, uploading to the mock server like GitHub's templated URL.
fn github_release(server: &MockServer, draft: bool) -> GitHubRelease {
    GitHubRelease {
        id: 1,
        tag_name: "v1.0.0".into(),
        draft,
        body: None,
        upload_url: format!("{}/upload{{?name,label}}", server.uri()),
    }
}

fn asset(id: u64, name: &str) -> Asset {
    Asset {
        id,
        name: name.into(),
        size: 1,
        state: "uploaded".into(),
    }
}

#[tokio::test]
async fn upload_replaces_only_the_same_named_asset() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/repos/owner/repo/releases/assets/7"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/repos/owner/repo/releases/assets/8"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/upload"))
        .and(query_param("name", "app.zip"))
        .and(header("content-type", "application/octet-stream"))
        .and(body_bytes(b"bytes".to_vec()))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"id": 9, "name": "app.zip", "size": 5, "state": "uploaded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let existing = [asset(7, "app.zip"), asset(8, "other.zip")];
    let uploaded = client(&server)
        .dist_upload(
            &github_release(&server, true),
            &existing,
            "app.zip",
            b"bytes".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(
        (uploaded.id, uploaded.name.as_str(), uploaded.size),
        (9, "app.zip", 5)
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn upload_refuses_published_releases() {
    let server = MockServer::start().await;
    let error = client(&server)
        .dist_upload(
            &github_release(&server, false),
            &[asset(7, "app.zip")],
            "app.zip",
            b"bytes".to_vec(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("refusing to upload to a published release"),
        "{error}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn finds_drafts_beyond_first_page() {
    let server = MockServer::start().await;
    let release = json!({"id": 1, "tag_name": "old", "draft": false, "body": null, "upload_url": "https://uploads.github.com/unused"});
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![release.clone(); 100]))
        .expect(1)
        .mount(&server)
        .await;
    let mut draft = release;
    draft["tag_name"] = json!("v1.0.0");
    draft["draft"] = json!(true);
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![draft]))
        .expect(1)
        .mount(&server)
        .await;
    assert!(client(&server).dist_release("v1.0.0").await.unwrap().draft);
}

#[tokio::test]
async fn stops_paging_releases_once_the_tag_is_found() {
    let server = MockServer::start().await;
    let mut releases = vec![
        json!({"id": 1, "tag_name": "old", "draft": false, "body": null, "upload_url": "https://uploads.github.com/unused"});
        100
    ];
    releases[50]["id"] = json!(2);
    releases[50]["tag_name"] = json!("v1.0.0");
    releases[50]["draft"] = json!(true);
    // A full page: a caller collecting every page would request page 2.
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(releases))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(0)
        .mount(&server)
        .await;
    let release = client(&server).dist_release("v1.0.0").await.unwrap();
    assert_eq!(release.id, 2);
    assert!(release.draft);
}

#[tokio::test]
async fn release_update_publishes_only_on_request_without_touching_prerelease() {
    for (publish, latest, expected) in [
        (
            true,
            Some(false),
            json!({"body": "notes", "draft": false, "make_latest": "false"}),
        ),
        // A draft kept by `git_release_draft` only receives the new body.
        (false, None, json!({"body": "notes"})),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/owner/repo/releases/1"))
            .and(body_json(&expected))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        client(&server)
            .dist_update_release(&github_release(&server, true), "notes", publish, latest)
            .await
            .unwrap();
    }
}

fn distribute_config() -> ReleaseConfig {
    ReleaseConfig::default()
        .with_git_only(true)
        .with_distribute(true)
}

/// Write the `publish = false` package `name` at `dir` with a single source file.
fn write_dist_package(dir: &Utf8Path, name: &str, source_file: &str) {
    fs_err::create_dir_all(dir.join("src")).unwrap();
    fs_err::write(dir.join(source_file), "fn main() {}\n").unwrap();
    fs_err::write(
        dir.join(cargo_utils::CARGO_TOML),
        crate::test_utils::package_manifest(name, "1.0.0", "publish = false\n"),
    )
    .unwrap();
}

/// Commit `repo` and tag it `v1.0.0`: the checkout a distribution job runs from.
fn tag_dist_repo(repo: &Repo, config: ReleaseConfig) -> ReleaseRequest {
    let metadata =
        cargo_utils::get_manifest_metadata(&repo.directory().join(cargo_utils::CARGO_TOML))
            .unwrap();
    repo.add_all_and_commit("initial").unwrap();
    repo.git(&["tag", "v1.0.0"]).unwrap();
    ReleaseRequest::new(metadata).with_default_package_config(config)
}

/// A committed, tagged `distribute = true` package whose only source file is `source_file`.
fn dist_fixture(source_file: &str) -> (tempfile::TempDir, Repo, ReleaseRequest) {
    let temporary = tempfile::tempdir().unwrap();
    let repo = Repo::init(temporary.path());
    write_dist_package(repo.directory(), "app", source_file);
    let request = tag_dist_repo(&repo, distribute_config());
    (temporary, repo, request)
}

fn new_dist_request_error(request: ReleaseRequest, client: GitClient) -> String {
    DistRequest::new(request, "v1.0.0".into(), client)
        .unwrap_err()
        .to_string()
}

#[tokio::test]
async fn dist_request_requires_one_tagged_binary_package_on_github() {
    let server = MockServer::start().await;
    let (_temporary, _repo, request) = dist_fixture("src/main.rs");
    DistRequest::new(request, "v1.0.0".into(), client(&server)).unwrap();

    let (_temporary, _repo, request) = dist_fixture("src/main.rs");
    let gitea = GitClient::new(GitForge::Gitea(crate::Gitea {
        remote: GitHub::new("owner".into(), "repo".into(), SecretString::from("token")).remote,
    }))
    .unwrap();
    let error = new_dist_request_error(request, gitea);
    assert!(error.contains("dist requires GitHub"), "{error}");

    let (_temporary, _repo, request) = dist_fixture("src/main.rs");
    let request = request.with_default_package_config(ReleaseConfig::default().with_git_only(true));
    let error = new_dist_request_error(request, client(&server));
    assert!(
        error.contains("tag `v1.0.0` must select exactly one package with distribute=true"),
        "{error}"
    );

    // Two packages sharing a tag template are ambiguous.
    let temporary = tempfile::tempdir().unwrap();
    let repo = Repo::init(temporary.path());
    fs_err::write(
        repo.directory().join(cargo_utils::CARGO_TOML),
        "[workspace]\nmembers = ['a', 'b']\nresolver = '2'\n",
    )
    .unwrap();
    for name in ["a", "b"] {
        write_dist_package(&repo.directory().join(name), name, "src/main.rs");
    }
    let shared_tag = GitTagConfig::enabled(true).set_name_template(Some("v{{ version }}".into()));
    let request = tag_dist_repo(&repo, distribute_config().with_git_tag(shared_tag));
    let error = new_dist_request_error(request, client(&server));
    assert!(
        error.contains("tag `v1.0.0` must select exactly one package with distribute=true"),
        "{error}"
    );

    let (_temporary, _repo, request) = dist_fixture("src/lib.rs");
    let error = new_dist_request_error(request, client(&server));
    assert!(error.contains("dist requires a binary target"), "{error}");
}

#[tokio::test]
async fn dist_request_requires_a_clean_checkout_of_the_tag() {
    let server = MockServer::start().await;
    let (_temporary, repo, request) = dist_fixture("src/main.rs");
    fs_err::write(repo.directory().join("src/main.rs"), "fn main() { }\n").unwrap();
    let error = new_dist_request_error(request, client(&server));
    assert!(
        error.contains("distribution requires a clean checkout of the release tag"),
        "{error}"
    );

    let (_temporary, repo, request) = dist_fixture("src/main.rs");
    fs_err::write(repo.directory().join("README.md"), "later\n").unwrap();
    repo.add_all_and_commit("after the tag").unwrap();
    let error = new_dist_request_error(request, client(&server));
    assert!(
        error.contains("HEAD must match release tag `v1.0.0`"),
        "{error}"
    );

    let (_temporary, repo, request) = dist_fixture("src/main.rs");
    repo.git(&["tag", "-d", "v1.0.0"]).unwrap();
    let error = new_dist_request_error(request, client(&server));
    assert!(error.contains("release tag is missing locally"), "{error}");
}

#[tokio::test]
async fn published_release_stops_build_and_is_a_finalize_no_op() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1, "tag_name": "v1.0.0", "draft": false, "body": "notes", "upload_url": "https://uploads.github.com/unused"
        }])))
        .mount(&server)
        .await;
    let (_temporary, _repo, request) = dist_fixture("src/main.rs");
    let request = DistRequest::new(request, "v1.0.0".into(), client(&server)).unwrap();
    // The check runs before cargo-dist is even looked up.
    let error = request
        .build(DistJob {
            run_id: "123".into(),
            index: 0,
            total: 1,
            target: "x86_64-unknown-linux-gnu".into(),
        })
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("release `v1.0.0` is already published"),
        "{error}"
    );
    request.finalize("123").await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(requests.iter().all(|request| request.method == "GET"));
}

#[tokio::test]
#[ignore = "requires cargo-dist 0.33.0 on PATH; builds a real binary"]
async fn real_cargo_dist_build_retry_and_finalize() {
    // Exercise the actual cargo-dist protocol against an in-memory GitHub release.
    use std::sync::{Arc, Mutex};
    let server = MockServer::start().await;
    let temporary = tempfile::tempdir().unwrap();
    let repo = Repo::init(temporary.path());
    fs_err::create_dir_all(repo.directory().join("src")).unwrap();
    // The dev profile unwinds, so only an inherited release profile compiles.
    fs_err::write(
        repo.directory().join("src/main.rs"),
        "#[cfg(not(panic = \"abort\"))]\ncompile_error!(\"must inherit release settings\");\nfn main() { println!(\"hello\"); }",
    )
    .unwrap();
    // No dist profile: both build and finalize must default to the release settings.
    fs_err::write(repo.directory().join("Cargo.toml"), "[package]\nname = \"dist-test\"\nversion = \"1.0.0\"\nedition = \"2021\"\npublish = false\n[profile.release]\npanic = \"abort\"\n").unwrap();
    fs_err::write(repo.directory().join(".gitignore"), "/target\n").unwrap();
    fs_err::write(
        repo.directory().join("CHANGELOG.md"),
        "# Changelog\n\n## [1.0.0] - 2026-01-01\n\nDuplicate changelog.\n",
    )
    .unwrap();
    let metadata =
        cargo_utils::get_manifest_metadata(&repo.directory().join("Cargo.toml")).unwrap();
    repo.add_all_and_commit("initial").unwrap();
    repo.git(&["tag", "v1.0.0"]).unwrap();
    let original = fs_err::read(repo.directory().join("Cargo.toml")).unwrap();

    #[derive(Default)]
    struct State {
        assets: BTreeMap<u64, (String, Vec<u8>)>,
        next_id: u64,
        published: Option<Value>,
        failed_global_upload: bool,
    }
    let state = Arc::new(Mutex::new(State::default()));
    let mock_state = Arc::clone(&state);
    let upload_url = format!("{}/upload{{?name,label}}", server.uri());
    Mock::given(|_: &wiremock::Request| true).respond_with(move |req: &wiremock::Request| {
        let mut state = mock_state.lock().unwrap();
        let asset_json = |id: u64, name: &str, bytes: &[u8]| json!({"id": id, "name": name, "size": bytes.len(), "state": "uploaded"});
        match (req.method.as_str(), req.url.path()) {
            ("GET", "/repos/owner/repo/releases") => ResponseTemplate::new(200).set_body_json(json!([{
                "id": 1, "tag_name": "v1.0.0", "draft": state.published.is_none(), "body": "Original changelog", "upload_url": upload_url
            }])),
            ("GET", "/repos/owner/repo/releases/1/assets") => ResponseTemplate::new(200).set_body_json(state.assets.iter().map(|(id, (name, bytes))| asset_json(*id, name, bytes)).collect::<Vec<_>>()),
            ("POST", "/upload") => {
                let name = req.url.query_pairs().find(|(k, _)| k == "name").unwrap().1.to_string();
                if name.contains("installer") && !state.failed_global_upload {
                    state.failed_global_upload = true;
                    return ResponseTemplate::new(422);
                }
                state.next_id += 1;
                let id = state.next_id;
                let response = asset_json(id, &name, &req.body);
                state.assets.insert(id, (name, req.body.clone()));
                ResponseTemplate::new(201).set_body_json(response)
            }
            ("PATCH", "/repos/owner/repo/releases/1") => {
                state.published = Some(serde_json::from_slice(&req.body).unwrap());
                ResponseTemplate::new(200)
            }
            (method, path) if path.starts_with("/repos/owner/repo/releases/assets/") => {
                let id: u64 = path.rsplit('/').next().unwrap().parse().unwrap();
                if method == "DELETE" {
                    state.assets.remove(&id);
                    ResponseTemplate::new(204)
                } else {
                    ResponseTemplate::new(200).set_body_bytes(state.assets[&id].1.clone())
                }
            }
            _ => ResponseTemplate::new(404),
        }
    }).mount(&server).await;
    let request = ReleaseRequest::new(metadata).with_default_package_config(
        ReleaseConfig::default()
            .with_git_only(true)
            .with_distribute(true),
    );
    let request = DistRequest::new(request, "v1.0.0".into(), client(&server)).unwrap();
    // The full error chain: `successful_status` keeps the HTTP status in the cause.
    async fn finalize_error(request: &DistRequest, run_id: &str) -> String {
        format!("{:#}", request.finalize(run_id).await.unwrap_err())
    }
    let error = finalize_error(&request, "123").await;
    assert!(
        error.contains("no successful distribution builds"),
        "{error}"
    );
    // The second build replaces the first one's assets and receipt.
    for _ in 0..2 {
        request
            .build(DistJob {
                run_id: "123".into(),
                index: 0,
                total: 1,
                target: cargo_dist::host_target().unwrap(),
            })
            .await
            .unwrap();
    }
    let error = finalize_error(&request, "124").await;
    assert!(
        error.contains("no successful distribution builds"),
        "{error}"
    );
    // The mock rejects the first installer upload.
    let error = finalize_error(&request, "123").await;
    assert!(error.contains("422"), "{error}");
    assert!(state.lock().unwrap().failed_global_upload);
    assert!(state.lock().unwrap().published.is_none());
    request.finalize("123").await.unwrap();
    request.finalize("123").await.unwrap();
    let state = state.lock().unwrap();
    let names: BTreeSet<&str> = state
        .assets
        .values()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names.len(), state.assets.len(), "{names:?}");
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with(&receipt_prefix("123")))
            .count(),
        1,
        "{names:?}"
    );
    let published = state.published.as_ref().unwrap();
    assert_eq!(published["draft"], false);
    let body = published["body"].as_str().unwrap();
    assert!(body.starts_with("Original changelog"));
    assert!(!body.contains("Duplicate changelog"));
    assert!(body.contains("## Download dist-test 1.0.0"));
    assert!(body.contains("## Install dist-test 1.0.0"));
    assert!(
        state
            .assets
            .values()
            .any(|(name, _)| name == "dist-manifest.json")
    );
    assert_eq!(
        fs_err::read(repo.directory().join("Cargo.toml")).unwrap(),
        original
    );
    assert!(!repo.directory().join("dist-workspace.toml").exists());
}

#[test]
#[ignore = "requires cargo-dist 0.33.0 on PATH; builds a real workspace binary"]
fn real_cargo_dist_selects_only_the_requested_workspace_package() {
    for profile_path in ["Cargo.toml", ".cargo/config.toml"] {
        check_workspace_dist_build(profile_path);
    }
}

/// A two-member workspace whose `unrelated` member must never be built, with the
/// `dist` profile declared in `profile_path`.
fn workspace_fixture(profile_path: &str) -> (tempfile::TempDir, Repo, Metadata, Project) {
    let temporary = tempfile::tempdir().unwrap();
    let repo = Repo::init(temporary.path());
    fs_err::write(
        repo.directory().join("Cargo.toml"),
        "[workspace]\nmembers = ['app', 'unrelated']\nresolver = '2'\n",
    )
    .unwrap();
    let profile_path = repo.directory().join(profile_path);
    fs_err::create_dir_all(profile_path.parent().unwrap()).unwrap();
    let existing = fs_err::read_to_string(&profile_path).unwrap_or_default();
    fs_err::write(
        &profile_path,
        format!("{existing}\n[profile.dist]\ninherits = 'dev'\nlto = false\n"),
    )
    .unwrap();
    fs_err::write(repo.directory().join(".gitignore"), "/target\n").unwrap();
    for (name, source) in [
        (
            "app",
            "#[cfg(not(debug_assertions))]\ncompile_error!(\"must preserve dist profile inheritance\");\nfn main() {}",
        ),
        (
            "unrelated",
            "compile_error!(\"must not build this package\");",
        ),
    ] {
        let dir = repo.directory().join(name);
        fs_err::create_dir_all(dir.join("src")).unwrap();
        fs_err::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = '{name}'\nversion = '1.0.0'\nedition = '2021'\npublish = false\n"
            ),
        )
        .unwrap();
        fs_err::write(dir.join("src/main.rs"), source).unwrap();
    }
    let metadata =
        cargo_utils::get_manifest_metadata(&repo.directory().join("Cargo.toml")).unwrap();
    repo.add_all_and_commit("workspace").unwrap();
    repo.git(&["tag", "app-v1.0.0"]).unwrap();
    let config = ReleaseRequest::new(metadata.clone())
        .with_default_package_config(ReleaseConfig::default().with_git_only(true));
    let project = Project::new(
        &repo.directory().join("Cargo.toml"),
        None,
        &HashSet::new(),
        &metadata,
        &config,
    )
    .unwrap();
    (temporary, repo, metadata, project)
}

fn prepare_app(project: &Project, metadata: &Metadata) -> anyhow::Result<CargoDist> {
    CargoDist::prepare(
        project,
        metadata,
        metadata.packages.iter().find(|p| p.name == "app").unwrap(),
        "https://github.com/owner/repo",
        "app-v1.0.0",
        vec![cargo_dist::host_target().unwrap()],
    )
}

fn check_workspace_dist_build(profile_path: &str) {
    let (_temporary, repo, metadata, project) = workspace_fixture(profile_path);
    let package = metadata.packages.iter().find(|p| p.name == "app").unwrap();
    let dist = prepare_app(&project, &metadata).unwrap();
    let manifest = dist.build(false).unwrap();
    manifest.validate("app-v1.0.0", package).unwrap();
    assert!(
        manifest
            .artifacts
            .keys()
            .all(|name| name.starts_with("app-"))
    );
    // Only files produced inside the temporary workspace may be uploaded.
    let error = dist
        .artifact_bytes(repo.directory().join("Cargo.toml").as_std_path())
        .unwrap_err()
        .to_string();
    assert!(error.contains("outside the temporary workspace"), "{error}");
    repo.is_clean().unwrap();
}

#[test]
#[ignore = "requires cargo-dist 0.33.0 on PATH; copies the workspace"]
fn real_cargo_dist_prepare_rejects_existing_dist_configuration() {
    let (_temporary, repo, metadata, project) = workspace_fixture("Cargo.toml");
    // The working tree, including untracked files, is what gets copied and prepared.
    let dist_workspace = repo.directory().join("dist-workspace.toml");
    fs_err::write(&dist_workspace, "[workspace]\nmembers = ['cargo:.']\n").unwrap();
    let error = prepare_app(&project, &metadata).unwrap_err().to_string();
    assert!(
        error.contains("manages its own cargo-dist configuration"),
        "{error}"
    );
    fs_err::remove_file(&dist_workspace).unwrap();
    // Any member's own cargo-dist metadata conflicts, not only the distributed one.
    let manifest = repo.directory().join("unrelated/Cargo.toml");
    let original = fs_err::read_to_string(&manifest).unwrap();
    fs_err::write(
        &manifest,
        format!("{original}\n[package.metadata.dist]\ndist = true\n"),
    )
    .unwrap();
    let error = prepare_app(&project, &metadata).unwrap_err().to_string();
    assert!(
        error.contains("cannot be combined with package.metadata.dist (unrelated)"),
        "{error}"
    );
    fs_err::write(&manifest, original).unwrap();
    prepare_app(&project, &metadata).unwrap();
    repo.is_clean().unwrap();
}
