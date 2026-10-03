use super::*;
use crate::{GitForge, GitHub, ReleaseConfig};
use secrecy::SecretString;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

fn package() -> Package {
    let metadata = fake_package::metadata::fake_metadata();
    metadata
        .packages
        .into_iter()
        .find(|p| metadata.workspace_members.contains(&p.id))
        .unwrap()
}

fn receipt(index: usize, total: usize) -> Receipt {
    let package = package();
    serde_json::from_value(json!({
        "schema": 1, "tag": "v1.0.0", "commit": "abc",
        "job": {"run_id": "123", "index": index, "total": total, "target": format!("target-{index}")},
        "assets": [{"id": index + 1, "name": format!("app-{index}.zip"), "size": 100, "state": "uploaded"}],
        "manifest": {
            "dist_version": cargo_dist::VERSION, "announcement_tag": "v1.0.0",
            "releases": [{"app_name": package.name, "app_version": package.version.to_string()}],
            "artifacts": {format!("app-{index}.zip"): {
                "name": format!("app-{index}.zip"), "kind": "executable-zip", "target_triples": [format!("target-{index}")]
            }}
        }
    })).unwrap()
}

fn assets(receipts: &[Receipt]) -> Vec<Asset> {
    receipts
        .iter()
        .flat_map(|r| &r.assets)
        .map(|a| Asset {
            id: a.id,
            name: a.name.clone(),
            size: a.size,
            state: a.state.clone(),
        })
        .collect()
}

#[test]
fn only_complete_matching_matrix_can_be_finalized() {
    let package = package();
    let mut receipts = vec![receipt(0, 2)];
    assert!(
        validate_receipts(
            &receipts,
            "123",
            "v1.0.0",
            "abc",
            &package,
            &assets(&receipts)
        )
        .is_err()
    );
    receipts.push(receipt(1, 2));
    assert_eq!(
        validate_receipts(
            &receipts,
            "123",
            "v1.0.0",
            "abc",
            &package,
            &assets(&receipts)
        )
        .unwrap(),
        ["target-0", "target-1"]
    );
    for (run, tag, commit) in [
        ("124", "v1.0.0", "abc"),
        ("123", "v2.0.0", "abc"),
        ("123", "v1.0.0", "def"),
    ] {
        assert!(
            validate_receipts(&receipts, run, tag, commit, &package, &assets(&receipts)).is_err()
        );
    }
    let mut replaced = assets(&receipts);
    replaced[0].id += 100;
    assert!(validate_receipts(&receipts, "123", "v1.0.0", "abc", &package, &replaced).is_err());
    receipts[1].job.target = receipts[0].job.target.clone();
    assert!(
        validate_receipts(
            &receipts,
            "123",
            "v1.0.0",
            "abc",
            &package,
            &assets(&receipts)
        )
        .is_err()
    );
}

#[test]
fn receipts_cannot_omit_artifacts_or_reuse_matrix_slots() {
    let package = package();
    let mut receipts = vec![receipt(0, 2), receipt(1, 2)];
    receipts[1].job.index = 0;
    assert!(
        validate_receipts(
            &receipts,
            "123",
            "v1.0.0",
            "abc",
            &package,
            &assets(&receipts)
        )
        .is_err()
    );
    receipts[1].job.index = 1;
    receipts[1].assets.clear();
    assert!(
        validate_receipts(
            &receipts,
            "123",
            "v1.0.0",
            "abc",
            &package,
            &assets(&receipts)
        )
        .is_err()
    );
}

#[test]
fn release_notes_preserve_existing_changelog() {
    let body = release_body("## Fixes\n\nFixed things.\n", "## Downloads\n\nA link.\n");
    assert!(body.starts_with("## Fixes\n\nFixed things.\n\n"));
    assert!(body.contains("## Downloads\n\nA link."));
    let mut manifest = receipt(0, 1).manifest;
    manifest.announcement_github_body =
        Some("## Release Notes\n\nDuplicate changelog.\n\n## Download app\n".into());
    manifest.extra.insert(
        "announcement_changelog".into(),
        json!("Duplicate changelog."),
    );
    assert_eq!(manifest.installation_notes().unwrap(), "## Download app\n");
}

fn client(server: &MockServer) -> GitClient {
    GitClient::new(GitForge::Github(
        GitHub::new("owner".into(), "repo".into(), SecretString::from("token"))
            .with_base_url(server.uri().parse().unwrap()),
    ))
    .unwrap()
}

#[tokio::test]
async fn finds_drafts_beyond_first_page() {
    let server = MockServer::start().await;
    let release = json!({"id": 1, "tag_name": "old", "draft": false, "body": null, "upload_url": "https://uploads.github.com/unused"});
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
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
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![draft]))
        .expect(1)
        .mount(&server)
        .await;
    assert!(client(&server).dist_release("v1.0.0").await.unwrap().draft);
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
    fs_err::write(
        repo.directory().join("src/main.rs"),
        "#[cfg(not(debug_assertions))]\ncompile_error!(\"must inherit release settings\");\nfn main() { println!(\"hello\"); }",
    )
    .unwrap();
    // No dist profile: both build and finalize must default to the release settings.
    fs_err::write(repo.directory().join("Cargo.toml"), "[package]\nname = \"dist-test\"\nversion = \"1.0.0\"\nedition = \"2021\"\npublish = false\n[profile.release]\ndebug-assertions = true\n").unwrap();
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
    let config = ReleaseRequest::new(metadata.clone())
        .with_default_package_config(ReleaseConfig::default().with_git_only(true).with_dist(true));
    let request = DistRequest::new(
        metadata,
        &config,
        "v1.0.0".into(),
        client(&server),
        "https://github.com/owner/repo".into(),
    )
    .unwrap();
    assert!(request.finalize("123").await.is_err());
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
    assert!(request.finalize("124").await.is_err());
    assert!(request.finalize("123").await.is_err());
    assert!(state.lock().unwrap().published.is_none());
    request.finalize("123").await.unwrap();
    request.finalize("123").await.unwrap();
    let state = state.lock().unwrap();
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

fn check_workspace_dist_build(profile_path: &str) {
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
    let package = metadata.packages.iter().find(|p| p.name == "app").unwrap();
    let targets = vec![cargo_dist::host_target().unwrap()];
    let dist = CargoDist::prepare(
        &project,
        &metadata,
        package,
        "https://github.com/owner/repo",
        &targets,
    )
    .unwrap();
    let manifest = dist.build("app-v1.0.0", &targets, false).unwrap();
    manifest.validate("app-v1.0.0", package).unwrap();
    assert!(
        manifest
            .artifacts
            .keys()
            .all(|name| name.starts_with("app-"))
    );
    repo.is_clean().unwrap();
}
