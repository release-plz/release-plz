use crate::RepoUrl;
use crate::git::forge::Remote;
use anyhow::{Context, bail};
use base64::Engine as _;
use git_cmd::Repo;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};
use url::Url;

/// Authenticate Git independently of credentials persisted by the checkout step.
/// Leave SSH and remotes pointing at a different repository unchanged.
pub(crate) fn authenticated_repo(repo: Repo, remote: &Remote) -> anyhow::Result<Repo> {
    let Ok(mut url) = Url::parse(&repo.original_remote_url()?) else {
        return Ok(repo);
    };
    let path = url.path().trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if !matches!(url.scheme(), "http" | "https")
        || url.origin() != remote.base_url.origin()
        || path != format!("/{}", remote.owner_slash_repo())
    {
        return Ok(repo);
    }
    // Keep the username so this scope overrides username-specific checkout headers.
    // The scope is passed through the environment because usernames can contain tokens.
    url.set_password(None).expect("HTTP URLs support passwords");
    url.set_query(None);
    url.set_fragment(None);
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("x-access-token:{}", remote.token.expose_secret()));
    let header = SecretString::from(format!("Authorization: Basic {credentials}"));
    Ok(repo.with_http_extra_header(url.into(), header))
}

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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    use super::*;

    fn remote(base_url: &str) -> Remote {
        Remote {
            base_url: base_url.parse().unwrap(),
            owner: "owner".into(),
            repo: "repo".into(),
            token: SecretString::from("release-token"),
        }
    }

    #[tokio::test]
    async fn authenticated_repo_authenticates_matching_http_remotes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let remote = remote(&format!("{}/api/v1/", server.uri()));
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("x-access-token:release-token")
        );
        for suffix in ["", "/", ".git", ".git/"] {
            let directory = tempdir().unwrap();
            let repo = Repo::init(&directory);
            let url = format!("{}/owner/repo{suffix}", server.uri());
            repo.git(&["remote", "add", "origin", &url]).unwrap();
            let repo = authenticated_repo(repo, &remote).unwrap();
            repo.fetch("main").unwrap_err();
            let requests = server.received_requests().await.unwrap();
            let authorization: Vec<_> = requests
                .last()
                .unwrap()
                .headers
                .get_all("authorization")
                .iter()
                .collect();
            assert_eq!(authorization, [expected.as_str()], "remote: {url}");
        }
        assert_eq!(server.received_requests().await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn authenticated_repo_redacts_credentials_without_modifying_config() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let directory = tempdir().unwrap();
        let repo = Repo::init(&directory);
        let mut url: Url = format!("{}/owner/repo.git", server.uri()).parse().unwrap();
        url.set_username("checkout-user").unwrap();
        url.set_password(Some("checkout-password")).unwrap();
        repo.git(&["remote", "add", "origin", url.as_str()])
            .unwrap();
        let config_before = repo.git(&["config", "--local", "--list"]).unwrap();
        let repo = authenticated_repo(repo, &remote(&server.uri())).unwrap();
        let debug = format!("{repo:?}");
        for secret in ["checkout-user", "checkout-password", "release-token"] {
            assert!(!debug.contains(secret), "debug exposed {secret}");
        }
        repo.fetch("main").unwrap_err();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("x-access-token:release-token")
        );
        let authorization: Vec<_> = requests[0]
            .headers
            .get_all("authorization")
            .iter()
            .collect();
        assert_eq!(authorization, [expected.as_str()]);
        assert_eq!(
            repo.git(&["config", "--local", "--list"]).unwrap(),
            config_before
        );
    }

    #[tokio::test]
    async fn authenticated_repo_does_not_authenticate_different_origins_or_repositories() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let base_url: Url = server.uri().parse().unwrap();
        let mut different_scheme = base_url.clone();
        different_scheme.set_scheme("https").unwrap();
        let mut different_port = base_url.clone();
        different_port
            .set_port(Some(if base_url.port() == Some(1) { 2 } else { 1 }))
            .unwrap();
        let mut different_host = base_url.clone();
        different_host.set_host(Some("localhost")).unwrap();
        for (api_url, repo_path) in [
            (different_scheme.as_str(), "owner/repo.git"),
            (different_port.as_str(), "owner/repo.git"),
            (different_host.as_str(), "owner/repo.git"),
            (base_url.as_str(), "owner/other.git"),
            (base_url.as_str(), "other/repo.git"),
            (base_url.as_str(), "owner/repository.git"),
        ] {
            let directory = tempdir().unwrap();
            let repo = Repo::init(&directory);
            let url = format!("{}/{repo_path}", server.uri());
            repo.git(&["remote", "add", "origin", &url]).unwrap();
            let repo = authenticated_repo(repo, &remote(api_url)).unwrap();
            repo.fetch("main").unwrap_err();
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 6);
        for request in requests {
            assert!(!request.headers.contains_key("authorization"));
        }
    }

    #[test]
    fn authenticated_repo_leaves_ssh_and_non_http_remotes_unchanged() {
        for url in [
            "git@example.com:owner/repo.git",
            "ssh://git@example.com/owner/repo.git",
            "git://example.com/owner/repo.git",
            "file:///owner/repo.git",
            "/owner/repo.git",
        ] {
            let directory = tempdir().unwrap();
            let repo = Repo::init(&directory);
            repo.git(&["remote", "add", "origin", url]).unwrap();
            let before = format!("{repo:?}");
            let repo = authenticated_repo(repo, &remote("https://example.com")).unwrap();
            assert_eq!(format!("{repo:?}"), before, "remote: {url}");
            assert_eq!(repo.original_remote_url().unwrap(), url);
        }
    }
}
