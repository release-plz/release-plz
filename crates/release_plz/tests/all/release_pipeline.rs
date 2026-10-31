use std::{
    os::unix::fs::PermissionsExt as _,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use git_cmd::Repo;
use release_plz_core::fs_utils::Utf8TempDir;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{body_partial_json, method, path_regex},
};

// Per-process fake Cargo keeps these tests independent of registries and does not
// change the test runner's environment. Marker files gate progress, not timings.
const CARGO_SCRIPT: &str = r#"#!/bin/sh
set -eu
wait_for() {
    attempts=0
    until test -f "$PIPELINE_STATE/$1"; do
        attempts=$((attempts + 1))
        if test "$attempts" -gt 1500; then
            echo "timed out waiting for $1" >&2
            exit 1
        fi
        sleep 0.01
    done
}
case "$1" in
    info)
        package=${2%@*}
        if test -f "$PIPELINE_STATE/$package-published"; then
            touch "$PIPELINE_STATE/$package-visible"
            exit 0
        fi
        echo "could not find $package" >&2
        exit 1
        ;;
    publish)
        shift
        dry_run=false
        while test "$#" -gt 0; do
            case "$1" in
                --package) shift; package=$1 ;;
                --dry-run) dry_run=true ;;
            esac
            shift
        done
        # Concurrent cargo publishes fail immediately.
        mkdir "$PIPELINE_STATE/publishing"
        trap 'rmdir "$PIPELINE_STATE/publishing"' EXIT
        if test "$dry_run" = false; then
            case "$package" in
                second) test -f "$PIPELINE_STATE/first-visible" ;;
                third) test -f "$PIPELINE_STATE/second-visible" ;;
            esac
        fi
        touch "$PIPELINE_STATE/$package-started"
        if test "$package" = second && test "$dry_run" = false; then
            if test "$PUBLISH_FAIL" = true; then
                wait_for first-git-started
                touch "$PIPELINE_STATE/second-failed"
                echo 'error: simulated publish failure' >&2
                exit 1
            fi
            wait_for first-git-finished
            wait_for allow-second-finish
        fi
        if test "$dry_run" = false; then
            touch "$PIPELINE_STATE/$package-published"
        fi
        echo Uploading >&2
        ;;
    *) exec "$REAL_CARGO" "$@" ;;
esac
"#;

struct Fixture {
    temporary: Utf8TempDir,
    repo: Repo,
    prepared_commit: String,
    original_commit: String,
}

impl Fixture {
    async fn new(server: &MockServer) -> Self {
        let temporary = Utf8TempDir::new().unwrap();
        let directory = temporary.path().join("repo");
        fs_err::create_dir(&directory).unwrap();
        let repo = Repo::init(&directory);
        fs_err::write(
            directory.join("Cargo.toml"),
            "[workspace]\nresolver = '3'\nmembers = ['third', 'second', 'first']\n",
        )
        .unwrap();
        for (package, dependency) in [
            ("first", None),
            ("second", Some("first")),
            ("third", Some("second")),
        ] {
            let package_dir = directory.join(package);
            fs_err::create_dir_all(package_dir.join("src")).unwrap();
            let dependency = dependency
                .map(|name| {
                    format!(
                        "[dependencies]\n{name} = {{ path = '../{name}', version = '0.1.0' }}\n"
                    )
                })
                .unwrap_or_default();
            fs_err::write(
                package_dir.join("Cargo.toml"),
                format!("[package]\nname = '{package}'\nversion = '0.1.0'\nedition = '2024'\n{dependency}"),
            )
            .unwrap();
            fs_err::write(package_dir.join("src/lib.rs"), "").unwrap();
        }
        repo.add_all_and_commit("chore: release").unwrap();
        let prepared_commit = repo.current_commit_hash().unwrap();
        fs_err::write(directory.join("README.md"), "a later commit").unwrap();
        repo.add_all_and_commit("docs: update readme").unwrap();
        let original_commit = repo.current_commit_hash().unwrap();
        let script = temporary.path().join("cargo");
        fs_err::write(&script, CARGO_SCRIPT).unwrap();
        fs_err::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        Mock::given(method("GET"))
            .and(path_regex(r"/repos/owner/repo/commits/.*/pulls$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "number": 1,
                "user": {"id": 1, "login": "release-plz"},
                "html_url": "https://github.com/owner/repo/pull/1",
                "head": {"ref": "release-plz-test", "sha": prepared_commit},
                "title": "chore: release",
                "labels": [],
            }])))
            .expect(1)
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"/repos/owner/repo/pulls/1/commits$"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!([{"sha": prepared_commit}])),
            )
            .expect(1)
            .mount(server)
            .await;

        Self {
            temporary,
            repo,
            prepared_commit,
            original_commit,
        }
    }

    fn start(&self, server: &MockServer, publish_fail: bool, dry_run: bool) -> Child {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("release-plz"));
        command
            .current_dir(self.repo.directory())
            .env("CARGO", self.temporary.path().join("cargo"))
            .env("REAL_CARGO", env!("CARGO"))
            .env("PIPELINE_STATE", self.temporary.path())
            .env("PUBLISH_FAIL", publish_fail.to_string())
            .env("RELEASE_PLZ_NO_ANSI", "1")
            .args([
                "release",
                "--git-token",
                "token",
                "--token",
                "token",
                "--forge",
                "github",
                "--output",
                "json",
                "--repo-url",
            ])
            .arg(format!("{}/owner/repo", server.uri()))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if dry_run {
            command.arg("--dry-run");
        }
        command.spawn().unwrap()
    }
}

async fn wait_for(path: &Path) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {}", path.display()));
}

// Wiremock responders are synchronous. The CLI runs in another process, and
// these tests use a multi-thread runtime so the test can open the other gate.
fn wait_for_blocking(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

async fn check_pipeline(git_fail: bool, publish_fail: bool) {
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server).await;
    let state = fixture.temporary.path();
    let state_for_tag = state.to_path_buf();
    let sha = fixture.prepared_commit.clone();
    Mock::given(method("POST"))
        .and(path_regex(r"/repos/owner/repo/git/tags$"))
        .respond_with(move |request: &Request| {
            let body: Value = request.body_json().unwrap();
            assert_eq!(body["object"], sha);
            let tag = body["tag"].as_str().unwrap();
            let package = tag.strip_suffix("-v0.1.0").unwrap();
            assert!(state_for_tag.join(format!("{package}-visible")).exists());
            if package == "second" {
                assert!(state_for_tag.join("first-git-finished").exists());
            } else if package == "third" {
                assert!(state_for_tag.join("second-git-finished").exists());
            }
            fs_err::write(state_for_tag.join(format!("{package}-tag")), "").unwrap();
            ResponseTemplate::new(201).set_body_json(json!({"sha": tag}))
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/repos/owner/repo/git/refs$"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    for package in ["first", "second", "third"] {
        let state = state.to_path_buf();
        Mock::given(method("POST"))
            .and(path_regex(r"/repos/owner/repo/releases$"))
            .and(body_partial_json(
                json!({"tag_name": format!("{package}-v0.1.0")}),
            ))
            .respond_with(move |_: &Request| {
                assert!(state.join(format!("{package}-tag")).exists());
                if package == "first" {
                    // Sequential execution cannot pass: the next publish must
                    // start before this Git release is allowed to finish.
                    wait_for_blocking(state.join("second-started").as_std_path());
                    fs_err::write(state.join("first-git-started"), "").unwrap();
                    if publish_fail {
                        wait_for_blocking(state.join("allow-first-finish").as_std_path());
                    }
                }
                fs_err::write(state.join(format!("{package}-git-finished")), "").unwrap();
                ResponseTemplate::new(if package == "first" && git_fail {
                    403
                } else {
                    201
                })
            })
            .expect(if package == "first" || (!git_fail && !publish_fail) {
                1
            } else {
                0
            })
            .mount(&server)
            .await;
    }
    let mut child = fixture.start(&server, publish_fail, false);
    wait_for(state.join("second-started").as_std_path()).await;
    let finished = if publish_fail {
        "second-failed"
    } else {
        "first-git-finished"
    };
    wait_for(state.join(finished).as_std_path()).await;
    // One stage finished (or failed), while the other is blocked. Neither
    // failure nor checkout restoration may cancel the operation still running.
    assert!(child.try_wait().unwrap().is_none());
    assert_eq!(
        fixture.repo.current_commit_hash().unwrap(),
        fixture.prepared_commit
    );
    assert!(!state.join("third-started").exists());
    fs_err::write(state.join("allow-first-finish"), "").unwrap();
    fs_err::write(state.join("allow-second-finish"), "").unwrap();
    let output = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
        .await
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        output.status.success(),
        !git_fail && !publish_fail,
        "{stderr}"
    );
    assert_eq!(
        fixture.repo.current_commit_hash().unwrap(),
        fixture.original_commit
    );
    assert_eq!(state.join("second-published").exists(), !publish_fail);
    assert_eq!(
        state.join("third-started").exists(),
        !git_fail && !publish_fail
    );
    if publish_fail {
        assert!(stderr.contains("simulated publish failure"), "{stderr}");
    }
    if git_fail {
        assert!(
            stderr.contains("failed to finalize release of first"),
            "{stderr}"
        );
    }
    if !git_fail && !publish_fail {
        let output: Value = serde_json::from_slice(&output.stdout).unwrap();
        let packages: Vec<_> = output["releases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|release| release["package_name"].as_str().unwrap())
            .collect();
        assert_eq!(packages, ["first", "second", "third"]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publishing_overlaps_git_releases_in_dependency_order() {
    check_pipeline(false, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_failure_drains_the_active_publish_before_restoring_checkout() {
    check_pipeline(true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_failure_preserves_the_previous_git_release() {
    check_pipeline(false, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_failures_report_both_errors() {
    check_pipeline(true, true).await;
}

#[tokio::test]
async fn dry_run_does_not_create_tags_or_releases() {
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server).await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&server)
        .await;
    let child = fixture.start(&server, false, true);
    let output = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"releases": []})
    );
    for package in ["first", "second", "third"] {
        assert!(
            fixture
                .temporary
                .path()
                .join(format!("{package}-started"))
                .exists()
        );
        assert!(
            !fixture
                .temporary
                .path()
                .join(format!("{package}-published"))
                .exists()
        );
    }
    assert_eq!(
        fixture.repo.current_commit_hash().unwrap(),
        fixture.original_commit
    );
}
