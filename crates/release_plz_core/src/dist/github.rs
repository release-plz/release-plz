use anyhow::{Context as _, ensure};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{ForgeType, GitClient, response_ext::ResponseExt as _};

#[derive(Debug, Deserialize)]
pub(super) struct Release {
    pub id: u64,
    pub tag_name: String,
    pub draft: bool,
    pub body: Option<String>,
    pub upload_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Asset {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub state: String,
}

impl GitClient {
    fn dist_url(&self, path: &str) -> String {
        format!(
            "{}/repos/{}/{}/{path}",
            self.remote.base_url.as_str().trim_end_matches('/'),
            self.remote.owner,
            self.remote.repo
        )
    }

    /// Draft releases do not emit Actions release events. Dispatch explicitly instead.
    pub(crate) async fn dispatch_dist(&self, tag: &str) -> anyhow::Result<()> {
        ensure!(self.forge == ForgeType::Github, "dist requires GitHub");
        self.client
            .post(self.dist_url("dispatches"))
            .json(&json!({"event_type": "release-plz-dist", "client_payload": {"tag": tag}}))
            .send()
            .await?
            .successful_status()
            .await?;
        Ok(())
    }

    pub(super) async fn dist_release(&self, tag: &str) -> anyhow::Result<Release> {
        // The by-tag endpoint only returns published releases. List releases to find drafts.
        for page in 1.. {
            let releases: Vec<Release> = self
                .client
                .get(self.dist_url(&format!("releases?per_page=100&page={page}")))
                .send()
                .await?
                .successful_status()
                .await?
                .json()
                .await?;
            let last = releases.len() < 100;
            if let Some(release) = releases.into_iter().find(|r| r.tag_name == tag) {
                return Ok(release);
            }
            ensure!(!last, "GitHub release for tag `{tag}` not found");
        }
        unreachable!()
    }

    pub(super) async fn dist_assets(&self, release: &Release) -> anyhow::Result<Vec<Asset>> {
        let mut assets = vec![];
        for page in 1.. {
            let batch: Vec<Asset> = self
                .client
                .get(self.dist_url(&format!(
                    "releases/{}/assets?per_page=100&page={page}",
                    release.id
                )))
                .send()
                .await?
                .successful_status()
                .await?
                .json()
                .await?;
            let last = batch.len() < 100;
            assets.extend(batch);
            if last {
                break;
            }
        }
        Ok(assets)
    }

    pub(super) async fn dist_download(&self, asset: &Asset) -> anyhow::Result<Vec<u8>> {
        Ok(self
            .client
            .get(self.dist_url(&format!("releases/assets/{}", asset.id)))
            .header(reqwest::header::ACCEPT, "application/octet-stream")
            .send()
            .await?
            .successful_status()
            .await?
            .bytes()
            .await?
            .to_vec())
    }

    pub(super) async fn dist_upload(
        &self,
        release: &Release,
        name: &str,
        bytes: Vec<u8>,
    ) -> anyhow::Result<Asset> {
        ensure!(release.draft, "refusing to upload to a published release");
        // Replacing an asset allows rerunning failed jobs. Distinct targets own distinct names.
        for asset in self.dist_assets(release).await? {
            if asset.name == name {
                self.client
                    .delete(self.dist_url(&format!("releases/assets/{}", asset.id)))
                    .send()
                    .await?
                    .successful_status()
                    .await?;
            }
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
        release: &Release,
        body: &str,
        latest: Option<bool>,
    ) -> anyhow::Result<()> {
        let mut payload = json!({"body": body, "draft": false});
        if let Some(latest) = latest {
            payload["make_latest"] = json!(latest.to_string());
        }
        self.client
            .patch(self.dist_url(&format!("releases/{}", release.id)))
            .json(&payload)
            .send()
            .await?
            .successful_status()
            .await?;
        Ok(())
    }
}
