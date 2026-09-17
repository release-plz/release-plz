use base64::prelude::*;
use secrecy::SecretString;
use serde_json::json;
use tempfile::tempdir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, body_partial_json, header, method, path, path_regex},
};

use super::*;
use crate::git::{forge::GitForge, github_client::GitHub};

fn github_client(server: &MockServer) -> GitClient {
    let github = GitHub::new("owner".into(), "repo".into(), SecretString::from("token"))
        .with_base_url(server.uri().parse().unwrap());
    GitClient::new(GitForge::Github(github)).unwrap()
}

async fn mock_github_push(server: &MockServer, base_sha: &str, patch_status: u16) {
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/git/refs"))
        .and(header("authorization", "Bearer token"))
        .and(body_partial_json(json!({"sha": base_sha})))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(header("authorization", "Bearer token"))
        .and(body_partial_json(json!({"variables": {"input": {
            "expectedHeadOid": base_sha,
            "message": {"headline": "chore: release v0.2.0"},
            "fileChanges": {
                "additions": [
                    {"path": "README.md", "contents": BASE64_STANDARD.encode("updated release")},
                    {"path": "CHANGELOG.md", "contents": BASE64_STANDARD.encode("new release notes")}
                ],
                "deletions": []
            }
        }}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"createCommitOnBranch": {"commit": {"oid": "new-release-sha"}}}
        })))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/repos/owner/repo/git/refs/heads/release-plz-test"))
        .and(header("authorization", "Bearer token"))
        .and(body_json(json!({"sha": "new-release-sha", "force": true})))
        .respond_with(ResponseTemplate::new(patch_status))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(
            r"^/repos/owner/repo/git/refs/heads/release-plz-test-tmp-\d+$",
        ))
        .and(header("authorization", "Bearer token"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(server)
        .await;
}

fn prepare_release_changes(repo: &Repo) {
    fs_err::write(repo.directory().join("README.md"), "updated release").unwrap();
    fs_err::write(repo.directory().join("CHANGELOG.md"), "new release notes").unwrap();
}

#[tokio::test]
async fn github_updates_existing_pr_without_git_remote_access() {
    test_logs::init();
    for local_pr_branch_exists in [false, true] {
        let temporary = tempdir().unwrap();
        let repo = Repo::init(temporary.path());
        // No remote is configured, so any git fetch or pull would fail. This also
        // covers checkouts whose PR branch has never been fetched locally.
        if local_pr_branch_exists {
            repo.checkout_new_branch("release-plz-test").unwrap();
            fs_err::write(repo.directory().join("README.md"), "old release").unwrap();
            repo.add_all_and_commit("chore: release v0.1.0").unwrap();
            repo.checkout_head().unwrap();
        }
        fs_err::write(repo.directory().join("feature.txt"), "new feature").unwrap();
        repo.add_all_and_commit("feat: new feature").unwrap();
        let base_sha = repo.current_commit_hash().unwrap();
        prepare_release_changes(&repo);

        let server = MockServer::start().await;
        mock_github_push(&server, &base_sha, 200).await;
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/pulls/42/commits"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"sha": "old-release-sha", "author": {"id": 1, "login": "release-plz[bot]"}}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/repos/owner/repo/pulls/42"))
            .and(body_partial_json(json!({"title": "chore: release v0.2.0"})))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let opened_pr: GitPr = serde_json::from_value(json!({
            "user": {"id": 1, "login": "release-plz[bot]"},
            "number": 42,
            "html_url": "https://github.com/owner/repo/pull/42",
            "head": {"ref": "release-plz-test", "sha": "old-release-sha"},
            "title": "chore: release v0.1.0",
            "body": "release notes",
            "labels": []
        }))
        .unwrap();
        let new_pr = Pr {
            base_branch: repo.original_branch().to_string(),
            branch: "release-plz-new".into(),
            title: "chore: release v0.2.0".into(),
            body: "release notes".into(),
            draft: false,
            labels: vec![],
        };
        let updated = handle_opened_pr(
            &github_client(&server),
            &opened_pr,
            &repo,
            &new_pr,
            DEFAULT_BRANCH_PREFIX,
        )
        .await
        .unwrap();

        assert_eq!(updated.number, opened_pr.number);
        assert_eq!(updated.head_branch, opened_pr.branch());
        assert_eq!(updated.base_branch, repo.original_branch());
        assert_eq!(repo.current_commit_hash().unwrap(), base_sha);
        assert_eq!(
            fs_err::read_to_string(repo.directory().join("feature.txt")).unwrap(),
            "new feature"
        );
        // Only the existing PR is edited; no PR is closed or opened.
        assert_eq!(server.received_requests().await.unwrap().len(), 6);
    }
}

#[tokio::test]
async fn github_cleans_up_temporary_branch_when_force_push_fails() {
    test_logs::init();
    let temporary = tempdir().unwrap();
    let repo = Repo::init(temporary.path());
    let base_sha = repo.current_commit_hash().unwrap();
    prepare_release_changes(&repo);
    let server = MockServer::start().await;
    mock_github_push(&server, &base_sha, 403).await;

    let error = github_force_push(
        &github_client(&server),
        "release-plz-test",
        "chore: release v0.2.0",
        &repo,
    )
    .await
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("failed to force push PR branch"),
        "{error:#}"
    );
}
