use crate::RepoUrl;
use crate::git::backend::Remote;
use anyhow::{Context, bail};
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Gitea {
    pub remote: Remote,
}

impl Gitea {
    pub fn new(url: RepoUrl, token: SecretString) -> anyhow::Result<Self> {
        match url.scheme.as_str() {
            "http" | "https" => {}
            _ => bail!(
                "invalid scheme for gitea url, only `http` and `https` are supported: {url:?}"
            ),
        }

        let base_url = url
            .gitea_api_url()
            .parse()
            .context("invalid Gitea API URL")?;
        Ok(Self {
            remote: Remote {
                base_url,
                owner: url.owner,
                repo: url.name,
                token,
            },
        })
    }

    pub fn default_headers(&self) -> anyhow::Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        let mut auth_header: HeaderValue = format!("token {}", self.remote.token.expose_secret())
            .parse()
            .context("invalid Gitea token")?;
        auth_header.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_header);
        Ok(headers)
    }
}

#[derive(Serialize, Deserialize)]
struct RepoPr {
    pub id: u64,
    pub head: Commit,
}

#[derive(Serialize, Deserialize)]
struct Commit {
    #[serde(rename = "ref")]
    pub ref_field: String,
}

#[derive(Serialize, Deserialize)]
struct EditPullRequest {
    state: &'static str,
}
