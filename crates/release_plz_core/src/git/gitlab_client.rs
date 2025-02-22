use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use tracing::debug;

use crate::{RepoUrl, git::backend::Remote};

#[derive(Debug, Clone)]
pub struct GitLab {
    pub remote: Remote,
}

impl GitLab {
    pub fn new(url: RepoUrl, token: SecretString) -> anyhow::Result<Self> {
        let base_url = url
            .gitlab_api_url()
            .parse()
            .context("invalid GitLab API URL")?;

        debug!("GitLab API URL: {base_url}");

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
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let mut private_token: HeaderValue = self
            .remote
            .token
            .expose_secret()
            .parse()
            .context("Invalid Gitlab token")?;
        private_token.set_sensitive(true);
        headers.insert("PRIVATE-TOKEN", private_token);

        Ok(headers)
    }
}
