use anyhow::Context;
use git_cmd::Repo;
use git_url_parse::{GitUrl, types::provider::GenericProvider};

use crate::ForgeType;

const GITHUB_COM: &str = "github.com";

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
        let git_host_url = if git_host_url.ends_with(".git") {
            git_host_url.to_string()
        } else {
            format!("{git_host_url}.git")
        };
        new_url(&git_host_url).with_context(|| format!("cannot parse git url {git_host_url}"))
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
        if self.host == GITHUB_COM {
            return format!("https://api.{GITHUB_COM}/");
        }

        format!("{}/api/v3/", self.github_enterprise_base_url())
    }

    pub fn github_graphql_url(&self) -> String {
        if self.host == GITHUB_COM {
            return format!("https://api.{GITHUB_COM}/graphql");
        }

        format!("{}/api/graphql", self.github_enterprise_base_url())
    }

    fn github_enterprise_base_url(&self) -> String {
        let scheme = self.scheme_ssh_as_https();
        let instance = match self.https_port() {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        };
        format!("{scheme}://{instance}")
    }

    fn scheme_ssh_as_https(&self) -> &str {
        if self.scheme == "ssh" {
            "https"
        } else {
            self.scheme.as_str()
        }
    }

    /// SSH remotes encode an SSH port (e.g. `ssh://git@host:2222/...`) that must
    /// not be reused for HTTPS traffic, so it is dropped in that case.
    fn https_port(&self) -> Option<u16> {
        match self.scheme.as_str() {
            "ssh" => None,
            _ => self.port,
        }
    }
}

fn new_url(git_host_url: &str) -> anyhow::Result<RepoUrl> {
    let git_url = GitUrl::parse(git_host_url)?;
    let provider: GenericProvider = git_url
        .provider_info()
        .context("cannot determine git provider")?;
    let host = git_url.host().context("cannot determine host")?.to_string();
    let scheme = git_url
        .scheme()
        .context("cannot determine scheme")?
        .to_string();
    let path = git_url
        .path()
        .strip_suffix(".git")
        .unwrap_or(git_url.path())
        .to_string();
    Ok(RepoUrl {
        owner: provider.owner().clone(),
        name: provider.repo().clone(),
        host,
        port: git_url.port(),
        scheme,
        path,
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
