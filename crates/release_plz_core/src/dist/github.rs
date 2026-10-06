use anyhow::{Context as _, ensure};
use reqwest::Url;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use crate::{ForgeType, GitClient, response_ext::ResponseExt as _};

#[derive(Debug, Deserialize)]
pub(super) struct GitHubRelease {
    pub id: u64,
    pub tag_name: String,
    pub draft: bool,
    pub body: Option<String>,
    pub upload_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct Asset {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub state: String,
}

impl GitClient {
    /// Draft releases do not emit Actions release events. Dispatch explicitly instead.
    pub(crate) async fn dispatch_dist(&self, tag: &str) -> anyhow::Result<()> {
        ensure!(self.forge == ForgeType::Github, "dist requires GitHub");
        self.client
            .post(format!("{}/dispatches", self.repo_url()))
            .json(&json!({"event_type": "release-plz-dist", "client_payload": {"tag": tag}}))
            .send()
            .await?
            .successful_status()
            .await?;
        Ok(())
    }

    /// Collect every page of a list endpoint relative to the repository URL.
    async fn dist_pages<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<Vec<T>> {
        let mut items = vec![];
        for page in 1.. {
            let batch: Vec<T> = self
                .client
                .get(format!(
                    "{}/{path}?per_page=100&page={page}",
                    self.repo_url()
                ))
                .send()
                .await?
                .successful_status()
                .await?
                .json()
                .await?;
            let last = batch.len() < 100;
            items.extend(batch);
            if last {
                break;
            }
        }
        Ok(items)
    }

    pub(super) async fn dist_release(&self, tag: &str) -> anyhow::Result<GitHubRelease> {
        // The by-tag endpoint only returns published releases. List releases to find drafts.
        self.dist_pages::<GitHubRelease>("releases")
            .await?
            .into_iter()
            .find(|release| release.tag_name == tag)
            .with_context(|| format!("GitHub release for tag `{tag}` not found"))
    }

    pub(super) async fn dist_assets(&self, release: &GitHubRelease) -> anyhow::Result<Vec<Asset>> {
        self.dist_pages(&format!("releases/{}/assets", release.id))
            .await
    }

    pub(super) async fn dist_download(&self, asset: &Asset) -> anyhow::Result<Vec<u8>> {
        Ok(self
            .client
            .get(format!("{}/releases/assets/{}", self.repo_url(), asset.id))
            .header(reqwest::header::ACCEPT, "application/octet-stream")
            .send()
            .await?
            .successful_status()
            .await?
            .bytes()
            .await?
            .to_vec())
    }

    /// Upload `bytes` as `name`, replacing any same-named asset among `existing`.
    pub(super) async fn dist_upload(
        &self,
        release: &GitHubRelease,
        existing: &[Asset],
        name: &str,
        bytes: Vec<u8>,
    ) -> anyhow::Result<Asset> {
        ensure!(release.draft, "refusing to upload to a published release");
        // Replacing an asset allows rerunning failed jobs. Distinct targets own distinct names.
        for asset in existing.iter().filter(|asset| asset.name == name) {
            self.client
                .delete(format!("{}/releases/assets/{}", self.repo_url(), asset.id))
                .send()
                .await?
                .successful_status()
                .await?;
        }
        let url = release
            .upload_url
            .split('{')
            .next()
            .context("missing upload URL")?;
        let mut url = Url::parse(url).context("invalid release upload URL")?;
        url.query_pairs_mut().append_pair("name", name);
        Ok(self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes)
            .send()
            .await?
            .successful_status()
            .await?
            .json()
            .await?)
    }

    pub(super) async fn dist_publish(
        &self,
        release: &GitHubRelease,
        body: &str,
        draft: bool,
        latest: Option<bool>,
    ) -> anyhow::Result<()> {
        let mut payload = json!({"body": body, "draft": draft});
        if let Some(latest) = latest {
            payload["make_latest"] = json!(latest.to_string());
        }
        self.client
            .patch(format!("{}/releases/{}", self.repo_url(), release.id))
            .json(&payload)
            .send()
            .await?
            .successful_status()
            .await?;
        Ok(())
    }
}
