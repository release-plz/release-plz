//! Helpers for creating a new Gitea repository for testing.

use std::process::Command;

use serde_json::json;

use crate::helpers::{fake_utils, reqwest_utils::ReqwestUtils};

use super::{CARGO_INDEX_REPO, GiteaContext, GiteaUser};

impl GiteaContext {
    pub async fn new(repo: String) -> Self {
        let client = reqwest::Client::new();
        let user = create_user();
        let token = create_token(&user, &client).await;

        create_repository(&token, &repo, &client).await;
        create_repository(&token, CARGO_INDEX_REPO, &client).await;
        upload_registry_config(&token, &user.username, &client).await;

        Self {
            user,
            token,
            repo,
            client,
        }
    }
}

pub async fn create_token(user: &GiteaUser, client: &reqwest::Client) -> String {
    #[derive(serde::Deserialize)]
    struct Token {
        sha1: String,
    }

    let token: Token = client
        .post(super::gitea_endpoint(&format!(
            "users/{}/tokens",
            user.username()
        )))
        .basic_auth(user.username(), Some(&user.password()))
        .json(&json!({
            "name": user.username(),
            "scopes": [
                // to create git tags and releases.
                "read:repository", "write:repository",
                // to create a new repository.
                "write:user",
                // to publish packages (i.e. cargo publish)
                "write:package",
                // to add a label to the release PR
                "read:issue", "write:issue"
            ]
        }))
        .send()
        .await
        .unwrap()
        .ok_if_2xx()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    token.sha1
}

async fn create_repository(user_token: &str, repo_name: &str, client: &reqwest::Client) {
    client
        .post(super::gitea_endpoint("user/repos"))
        .query(&[("token", user_token)])
        .json(&json!({
            "name": repo_name,
            // Automatically initialize the repository
            "auto_init": true,
        }))
        .send()
        .await
        .expect("Failed to create repository")
        .ok_if_2xx()
        .await
        .unwrap();
}

async fn upload_registry_config(user_token: &str, username: &str, client: &reqwest::Client) {
    use base64::Engine as _;
    let content = {
        let cargo_url = format!("http://localhost:3000/api/packages/{username}/cargo");
        format!("{{\"dl\":\"{cargo_url}/api/v1/crates\",\"api\":\"{cargo_url}\"}}")
    };

    client
        .post(super::gitea_endpoint(&format!(
            "repos/{username}/{CARGO_INDEX_REPO}/contents/config.json"
        )))
        .query(&[("token", user_token)])
        .json(&json!({
            "message": "Add config.json",
            "content": base64::engine::general_purpose::STANDARD.encode(content.as_bytes()),
        }))
        .send()
        .await
        .expect("Failed to upload content")
        .ok_if_2xx()
        .await
        .unwrap();
}

fn run_create_user_command(user: &GiteaUser) {
    Command::new("docker")
        .arg("exec")
        .arg("gitea")
        .arg("gitea")
        .arg("admin")
        .arg("user")
        .arg("create")
        .arg("--username")
        .arg(user.username())
        .arg("--password")
        .arg(user.password())
        .arg("--email")
        .arg(user.email())
        .arg("--must-change-password=false")
        .status()
        .expect("Failed to create user. Is the docker daemon running?");
}

/// Create a random user and return it's username and passoword.
pub fn create_user() -> GiteaUser {
    let user = GiteaUser {
        username: fake_utils::fake_id(),
        password: "psw".to_string(),
    };
    run_create_user_command(&user);
    user
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn can_create_gitea_repository() {
    let repo_name = "myrepo";
    let gitea = GiteaContext::new(repo_name.to_string()).await;
    assert!(gitea.repo_exists(repo_name).await);
}
