use crate::RepoUrl;
use crate::git::forge::Remote;
use anyhow::{Context, bail};
use reqwest::Url;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};

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

        let base_url = resolve_base_url(&url)?;
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

fn resolve_base_url(url: &RepoUrl) -> anyhow::Result<Url> {
    let mut api_url = url.gitea_api_url();

    // In Gitea/Forgejo actions, checkout can leave an `origin` that still points to github.com
    // while the actual forge host is exposed via GITHUB_SERVER_URL.
    if url.host == "github.com"
        && let Ok(server_url) = std::env::var("GITHUB_SERVER_URL")
        && let Ok(parsed_server_url) = Url::parse(&server_url)
        && parsed_server_url.host_str() != Some("github.com")
    {
        let mut server = parsed_server_url;
        server.set_query(None);
        server.set_fragment(None);
        server.set_path("/api/v1/");
        api_url = server.to_string();
    }

    api_url.parse().context("invalid Gitea API URL")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::resolve_base_url;
    use crate::RepoUrl;

    static NO_PARALLEL: Mutex<()> = Mutex::new(());

    fn with_github_server_url(value: Option<&str>, test: impl FnOnce()) {
        let _guard = NO_PARALLEL.lock().unwrap();
        let old = std::env::var("GITHUB_SERVER_URL").ok();

        match value {
            Some(v) => unsafe { std::env::set_var("GITHUB_SERVER_URL", v) },
            None => unsafe { std::env::remove_var("GITHUB_SERVER_URL") },
        }

        test();

        match old {
            Some(v) => unsafe { std::env::set_var("GITHUB_SERVER_URL", v) },
            None => unsafe { std::env::remove_var("GITHUB_SERVER_URL") },
        }
    }

    #[test]
    fn uses_repo_host_for_gitea_api_by_default() {
        with_github_server_url(None, || {
            let repo = RepoUrl::new("https://git.cscherr.de/PlexSheep/rough2").unwrap();
            let api = resolve_base_url(&repo).unwrap();
            assert_eq!(api.as_str(), "https://git.cscherr.de/api/v1/");
        });
    }

    #[test]
    fn uses_github_server_url_when_origin_host_is_github() {
        with_github_server_url(Some("https://git.cscherr.de"), || {
            let repo = RepoUrl::new("https://github.com/PlexSheep/rough2").unwrap();
            let api = resolve_base_url(&repo).unwrap();
            assert_eq!(api.as_str(), "https://git.cscherr.de/api/v1/");
        });
    }
}
