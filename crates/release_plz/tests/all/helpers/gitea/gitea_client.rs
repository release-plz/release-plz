use reqwest::{IntoUrl, RequestBuilder, Response};
use serde::de::DeserializeOwned;

use crate::helpers::{gitea::gitea_address, reqwest_utils::ReqwestUtils};

use super::{CARGO_INDEX_REPO, GiteaContext, GiteaUser};

impl GiteaContext {
    pub fn repo_clone_url(&self) -> String {
        format!(
            // if you need ssh instead of http: "ssh://git@localhost:2222/{}/{}.git",
            "http://{}:{}@{}/{}/{}.git",
            self.user.username(),
            self.user.password(),
            gitea_address(),
            self.user.username(),
            self.repo
        )
    }

    /// Index of the Cargo registry that gitea hosts for the user.
    /// See <https://docs.gitea.com/usage/packages/cargo>.
    pub fn cargo_index_url(&self) -> String {
        format!(
            "http://{}/{}/{CARGO_INDEX_REPO}.git",
            gitea_address(),
            self.user.username()
        )
    }

    /// The same registry as [`Self::cargo_index_url`], served over the sparse protocol.
    pub fn cargo_sparse_index_url(&self) -> String {
        let registry_url = super::cargo_registry_url(self.user.username());
        format!("sparse+{registry_url}/")
    }

    pub async fn repo_exists(&self, repo_name: &str) -> bool {
        let repo = self.get_repo(repo_name).await;
        repo == repo_name
    }

    fn pull_url(&self, pr_number: u64) -> String {
        format!("{}/pulls/{}", self.repo_url(), pr_number)
    }

    fn repo_url(&self) -> String {
        self.specific_repo_url(&self.repo)
    }

    fn specific_repo_url(&self, repo_name: &str) -> String {
        super::gitea_endpoint(&format!("repos/{}/{}", self.user.username, repo_name))
    }

    /// Send the request authenticated as the user and fail if the response isn't successful.
    async fn send_as_user(&self, request: RequestBuilder) -> anyhow::Result<Response> {
        request
            .basic_auth(self.user.username(), Some(self.user.password()))
            .send()
            .await
            .unwrap()
            .ok_if_2xx()
            .await
    }

    /// Get `url` as the user and deserialize the JSON response.
    async fn get_json<T: DeserializeOwned>(&self, url: impl IntoUrl) -> T {
        self.send_as_user(self.client.get(url))
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// Get the repository and return its name.
    async fn get_repo(&self, repo_name: &str) -> String {
        let repo: Repository = self.get_json(self.specific_repo_url(repo_name)).await;
        repo.name
    }

    pub async fn changed_files_in_pr(&self, pr_number: u64) -> Vec<ChangedFile> {
        let pr_url = format!("{}/files", self.pull_url(pr_number));
        self.get_json(pr_url).await
    }

    pub async fn merge_pr_retrying(&self, pr_number: u64) {
        let max_retries = 20;
        let mut retries = 0;
        loop {
            match self.merge_pr(pr_number).await {
                Ok(()) => break,
                Err(e) => {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    retries += 1;
                    assert!(
                        retries <= max_retries,
                        "Failed to merge PR after {max_retries} retries. Error: {e:?}"
                    );
                }
            }
        }
    }

    async fn merge_pr(&self, pr_number: u64) -> anyhow::Result<()> {
        let pr_url = format!("{}/merge", self.pull_url(pr_number));
        // set merge strategy
        let request = self
            .client
            .post(pr_url)
            .json(&serde_json::json!({"Do": "squash"}));
        self.send_as_user(request).await?;
        Ok(())
    }

    /// Get the Gitea release associated to the given `tag`.
    pub async fn get_gitea_release(&self, tag: &str) -> GiteaRelease {
        let request_path = format!("{}/releases/tags/{}", self.repo_url(), tag);
        self.get_json(request_path).await
    }

    pub async fn get_file_content(&self, branch: &str, file_path: &str) -> String {
        use base64::Engine as _;
        let request_path = format!("{}/contents/{}", self.repo_url(), file_path);
        let mut url = reqwest::Url::parse(&request_path).unwrap();
        url.query_pairs_mut().append_pair("ref", branch);
        let response: Contents = self.get_json(url).await;
        let content = base64::engine::general_purpose::STANDARD
            .decode(response.content.as_bytes())
            .unwrap();
        String::from_utf8(content).unwrap()
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct GiteaRelease {
    pub name: String,
    pub body: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct Contents {
    pub content: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ChangedFile {
    pub filename: String,
}

impl GiteaUser {
    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }
}

#[derive(serde::Deserialize)]
struct Repository {
    name: String,
}
