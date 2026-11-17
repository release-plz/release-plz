use crate::RepoUrl;
use crate::git::forge::Remote;
use anyhow::{Context, bail};
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct Gitea {
    pub remote: Remote,
}

impl Gitea {
    /// # Errors
    ///
    /// Returns an error if the repository uses a scheme other than HTTP or HTTPS, or the API
    /// URL is invalid.
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

    /// # Errors
    ///
    /// Returns an error if the token cannot be represented in an HTTP authorization header.
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
