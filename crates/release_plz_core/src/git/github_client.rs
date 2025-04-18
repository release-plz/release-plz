use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::git::forge::Remote;

#[derive(Debug, Clone)]
pub struct GitHub {
    pub remote: Remote,
}

impl GitHub {
    pub fn new(owner: String, repo: String, token: SecretString) -> Self {
        Self {
            remote: Remote {
                owner,
                repo,
                token,
                base_url: "https://api.github.com".parse().unwrap(),
            },
        }
    }

    pub fn with_base_url(self, base_url: Url) -> Self {
        Self {
            remote: Remote {
                base_url,
                ..self.remote
            },
        }
    }

    pub fn default_headers(&self) -> anyhow::Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        let mut auth_header: HeaderValue = format!("Bearer {}", self.remote.token.expose_secret())
            .parse()
            .context("invalid GitHub token")?;
        auth_header.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_header);
        Ok(headers)
    }
}
