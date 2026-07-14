use anyhow::{Context, bail};
use git_cmd::Repo;
use url::Url;

use crate::ForgeType;

const GITHUB_COM: &str = "github.com";
const GITHUB_COM_SSH: &str = "ssh.github.com";
const GITHUB_COM_WWW: &str = "www.github.com";

#[derive(Debug, Clone)]
pub struct RepoUrl {
    pub scheme: String,
    pub host: String,
    port: Option<u16>,
    pub owner: String,
    pub name: String,
    pub path: String,
}

impl RepoUrl {
    pub fn new(git_host_url: &str) -> anyhow::Result<Self> {
        new_url(git_host_url).with_context(|| format!("cannot parse git url {git_host_url}"))
    }

    pub fn from_repo(repo: &Repo) -> Result<Self, anyhow::Error> {
        let url = repo
            .original_remote_url()
            .context("cannot determine origin url")?;
        Self::new(&url)
    }

    pub fn is_on_github(&self) -> bool {
        self.host.contains("github")
    }

    /// Whether this repository uses one of GitHub.com's public Git hostnames.
    pub fn is_on_github_dot_com(&self) -> bool {
        matches!(
            self.host.as_str(),
            GITHUB_COM | GITHUB_COM_SSH | GITHUB_COM_WWW
        )
    }

    pub fn full_host(&self) -> String {
        let instance = match self.https_port() {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        };
        format!("https://{instance}/{}/{}", self.owner, self.name)
    }

    /// Get GitHub/Gitea release link
    pub fn git_release_link(&self, prev_tag: &str, new_tag: &str) -> String {
        let host = self.full_host();

        if prev_tag == new_tag {
            format!("{host}/releases/tag/{new_tag}")
        } else {
            format!("{host}/compare/{prev_tag}...{new_tag}")
        }
    }

    pub fn git_pr_link_for(&self, forge: ForgeType) -> String {
        let host = self.full_host();
        let pull_path = match forge {
            ForgeType::Github => "pull",
            ForgeType::Gitea | ForgeType::Gitlab => "pulls",
        };
        format!("{host}/{pull_path}")
    }

    pub fn gitea_api_url(&self) -> String {
        let v1 = "api/v1/";
        if let Some(port) = self.port {
            format!("{}://{}:{}/{v1}", self.scheme, self.host, port)
        } else {
            format!("{}://{}/{v1}", self.scheme, self.host)
        }
    }

    pub fn gitlab_api_url(&self) -> String {
        let v4 = "api/v4/projects";
        let prj_path = urlencoding::encode(self.path.strip_prefix('/').unwrap_or(&self.path));
        let scheme = if self.scheme == "ssh" {
            "https"
        } else {
            self.scheme.as_str()
        };
        if let Some(port) = self.port {
            format!("{scheme}://{}:{port}/{v4}/{prj_path}", self.host)
        } else {
            format!("{scheme}://{}/{v4}/{prj_path}", self.host)
        }
    }

    pub fn github_api_url(&self) -> String {
        if self.is_on_github_dot_com() {
            return format!("https://api.{GITHUB_COM}/");
        }

        format!("{}/api/v3/", self.github_enterprise_base_url())
    }

    pub fn github_graphql_url(&self) -> String {
        if self.is_on_github_dot_com() {
            return format!("https://api.{GITHUB_COM}/graphql");
        }

        format!("{}/api/graphql", self.github_enterprise_base_url())
    }

    fn github_enterprise_base_url(&self) -> String {
        let scheme = self.api_scheme();
        let instance = match self.https_port() {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        };
        format!("{scheme}://{instance}")
    }

    fn api_scheme(&self) -> &str {
        if self.scheme.eq_ignore_ascii_case("http") {
            "http"
        } else {
            "https"
        }
    }

    /// Only HTTP(S) remotes encode a web/API port. Ports from other Git transports
    /// (e.g. SSH port 2222 or Git port 9418) must not be reused for HTTPS traffic.
    fn https_port(&self) -> Option<u16> {
        if self.scheme.eq_ignore_ascii_case("http") || self.scheme.eq_ignore_ascii_case("https") {
            self.port
        } else {
            None
        }
    }
}

fn new_url(git_host_url: &str) -> anyhow::Result<RepoUrl> {
    match Url::parse(git_host_url) {
        Ok(git_url) if git_url.has_host() => repo_url_from_url(&git_url),
        _ => new_scp_url(git_host_url),
    }
}

fn new_scp_url(git_host_url: &str) -> anyhow::Result<RepoUrl> {
    let separator = git_host_url
        .find("]:")
        .map_or_else(|| git_host_url.find(':'), |index| Some(index + 1))
        .context("cannot determine host")?;
    let (authority, path) = git_host_url.split_at(separator);
    let path = path
        .strip_prefix(':')
        .context("cannot determine repository path")?;
    if authority.contains('/') || authority.is_empty() || path.is_empty() {
        bail!("invalid SCP-style git URL");
    }

    let git_url = Url::parse(&format!("ssh://{authority}/{path}"))?;
    let host = git_url.host_str().context("cannot determine host")?;
    repo_url_from_parts(host, git_url.port(), git_url.scheme(), path)
}

fn repo_url_from_url(git_url: &Url) -> anyhow::Result<RepoUrl> {
    let host = git_url.host_str().context("cannot determine host")?;
    repo_url_from_parts(host, git_url.port(), git_url.scheme(), git_url.path())
}

fn repo_url_from_parts(
    host: &str,
    port: Option<u16>,
    scheme: &str,
    path: &str,
) -> anyhow::Result<RepoUrl> {
    let path = path.strip_suffix(".git").unwrap_or(path);
    let provider_path = path.strip_prefix('/').unwrap_or(path);
    let (owner, name) = provider_path
        .split_once('/')
        .context("cannot determine git provider")?;
    if owner.is_empty() || name.is_empty() {
        bail!("cannot determine git provider");
    }

    Ok(RepoUrl {
        owner: owner.to_string(),
        name: name.to_string(),
        host: host.to_string(),
        port,
        scheme: scheme.to_string(),
        path: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::RepoUrl;
    use crate::ForgeType;

    const GITHUB_REPO_URL: &str = "https://github.com/release-plz/release-plz";

    #[test]
    fn gh_release_link_works_for_first_release() {
        let repo = RepoUrl::new(GITHUB_REPO_URL).unwrap();
        let tag = "v0.0.1";
        let expected_url = format!("{GITHUB_REPO_URL}/releases/tag/{tag}");
        // when we are at the first release, we have the prev_tag and the new_tag to be
        // the same as there is no other tag available.
        let release_link = repo.git_release_link(tag, tag);
        assert_eq!(expected_url, release_link);
    }

    #[test]
    fn gh_release_link_for_crates_already_published() {
        let repo = RepoUrl::new(GITHUB_REPO_URL).unwrap();
        let previous_tag = "v0.1.0";
        let next_tag = "v0.5.0";
        // when there is already a previous version, we should use the compare url, with the
        // ranging between the previous tag and the newest one
        let expected_url = format!("{GITHUB_REPO_URL}/compare/{previous_tag}...{next_tag}");
        let release_link = repo.git_release_link(previous_tag, next_tag);
        assert_eq!(expected_url, release_link);
    }

    #[test]
    fn gitlab_api_url() {
        let git_repo = RepoUrl::new("git@host.example.com:ab/cd/myproj.git").unwrap();
        assert_eq!(
            "https://host.example.com/api/v4/projects/ab%2Fcd%2Fmyproj",
            git_repo.gitlab_api_url()
        );

        let http_repo = RepoUrl::new("https://host.example.com/ab/cd/myproj.git").unwrap();
        assert_eq!(
            "https://host.example.com/api/v4/projects/ab%2Fcd%2Fmyproj",
            http_repo.gitlab_api_url()
        );
    }

    #[test]
    fn scp_style_url_parts() {
        let repo = RepoUrl::new("git@host.example.com:ab/cd/myproj.git").unwrap();
        assert_eq!(repo.scheme, "ssh");
        assert_eq!(repo.host, "host.example.com");
        assert_eq!(repo.port, None);
        assert_eq!(repo.owner, "ab");
        assert_eq!(repo.name, "cd/myproj");
        assert_eq!(repo.path, "ab/cd/myproj");
    }

    #[test]
    fn github_api_url_dotcom() {
        let r = RepoUrl::new("https://github.com/owner/repo").unwrap();
        assert_eq!(r.github_api_url(), "https://api.github.com/");
        assert_eq!(r.github_graphql_url(), "https://api.github.com/graphql");
    }

    #[test]
    fn github_api_url_dotcom_http_still_uses_https() {
        let r = RepoUrl::new("http://github.com/owner/repo").unwrap();
        assert_eq!(r.github_api_url(), "https://api.github.com/");
        assert_eq!(r.github_graphql_url(), "https://api.github.com/graphql");
    }

    #[test]
    fn github_api_url_dotcom_aliases() {
        for url in [
            "ssh://git@ssh.github.com:443/owner/repo.git",
            "https://www.github.com/owner/repo.git",
        ] {
            let r = RepoUrl::new(url).unwrap();
            assert_eq!(r.github_api_url(), "https://api.github.com/");
            assert_eq!(r.github_graphql_url(), "https://api.github.com/graphql");
        }
    }

    #[test]
    fn github_api_url_enterprise_https() {
        let r = RepoUrl::new("https://github.example.com/owner/repo").unwrap();
        assert_eq!(r.github_api_url(), "https://github.example.com/api/v3/");
        assert_eq!(
            r.github_graphql_url(),
            "https://github.example.com/api/graphql"
        );
    }

    #[test]
    fn github_api_url_enterprise_ssh_origin() {
        // SSH origins must be promoted to HTTPS for the REST/GraphQL APIs.
        let r = RepoUrl::new("git@github.example.com:owner/repo.git").unwrap();
        assert_eq!(r.github_api_url(), "https://github.example.com/api/v3/");
        assert_eq!(
            r.github_graphql_url(),
            "https://github.example.com/api/graphql"
        );
    }

    #[test]
    fn github_enterprise_https_port_is_kept_for_api_and_web_links() {
        let r = RepoUrl::new("https://github.example.com:8443/owner/repo").unwrap();
        assert_eq!(r.full_host(), "https://github.example.com:8443/owner/repo");
        assert_eq!(
            r.github_api_url(),
            "https://github.example.com:8443/api/v3/"
        );
        assert_eq!(
            r.github_graphql_url(),
            "https://github.example.com:8443/api/graphql"
        );
    }

    #[test]
    fn github_enterprise_ssh_port_is_not_reused_for_https() {
        let r = RepoUrl::new("ssh://git@github.example.com:2222/owner/repo.git").unwrap();
        assert_eq!(r.full_host(), "https://github.example.com/owner/repo");
        assert_eq!(r.github_api_url(), "https://github.example.com/api/v3/");
        assert_eq!(
            r.github_graphql_url(),
            "https://github.example.com/api/graphql"
        );
    }

    #[test]
    fn github_enterprise_git_transport_uses_https_without_git_port() {
        let r = RepoUrl::new("git://github.example.com:9418/owner/repo.git").unwrap();
        assert_eq!(r.full_host(), "https://github.example.com/owner/repo");
        assert_eq!(r.github_api_url(), "https://github.example.com/api/v3/");
        assert_eq!(
            r.github_graphql_url(),
            "https://github.example.com/api/graphql"
        );
    }

    #[test]
    fn git_pr_link_uses_pull_for_github_including_enterprise() {
        let dotcom = RepoUrl::new("https://github.com/owner/repo").unwrap();
        assert_eq!(
            dotcom.git_pr_link_for(ForgeType::Github),
            "https://github.com/owner/repo/pull"
        );

        // GitHub Enterprise host without "github" in the name still uses `/pull`.
        let ghes = RepoUrl::new("https://git.company.com/org/repo").unwrap();
        assert_eq!(
            ghes.git_pr_link_for(ForgeType::Github),
            "https://git.company.com/org/repo/pull"
        );
    }

    #[test]
    fn git_pr_link_uses_pulls_for_gitea() {
        let r = RepoUrl::new("https://gitea.example.com/owner/repo").unwrap();
        assert_eq!(
            r.git_pr_link_for(ForgeType::Gitea),
            "https://gitea.example.com/owner/repo/pulls"
        );
    }
}
