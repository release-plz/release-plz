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
    // Credentials in the URL are not part of the scope, and must not appear in arguments.
    url.set_username("").expect("HTTP URLs support usernames");
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
