use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::RepoUrl;
use crate::git::forge::Remote;

#[derive(Debug, Clone)]
pub struct GitHub {
    pub remote: Remote,
    /// Whether the repository is hosted on GitHub Enterprise Server rather than GitHub.com.
    enterprise: bool,
}

impl GitHub {
    pub fn from_repo_url(url: RepoUrl, token: SecretString) -> anyhow::Result<Self> {
        let base_url = url
            .github_api_url()
            .parse()
            .context("invalid GitHub REST API URL derived from repo origin")?;
        Ok(Self {
            enterprise: !url.is_on_github_dot_com(),
            remote: Remote {
                owner: url.owner,
                repo: url.name,
                token,
                base_url,
            },
        })
    }

    pub fn new(owner: String, repo: String, token: SecretString) -> Self {
        Self {
            remote: Remote {
                owner,
                repo,
                token,
                base_url: "https://api.github.com".parse().unwrap(),
            },
            enterprise: false,
        }
    }

    pub fn with_base_url(self, base_url: Url) -> Self {
        Self {
            remote: Remote {
                base_url,
                ..self.remote
            },
            ..self
        }
    }

    pub fn is_enterprise(&self) -> bool {
        self.enterprise
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
