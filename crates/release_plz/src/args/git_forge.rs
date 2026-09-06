use clap::ValueEnum;
use release_plz_core::{ForgeType, GitForge, GitHub, GitLab, Gitea, RepoUrl};
use secrecy::SecretString;

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GitForgeKind {
    #[value(name = "github")]
    Github,
    #[value(name = "gitea")]
    Gitea,
    #[value(name = "gitlab")]
    Gitlab,
}

impl From<GitForgeKind> for ForgeType {
    fn from(kind: GitForgeKind) -> Self {
        match kind {
            GitForgeKind::Github => Self::Github,
            GitForgeKind::Gitea => Self::Gitea,
            GitForgeKind::Gitlab => Self::Gitlab,
        }
    }
}

pub(super) fn git_forge(
    repo: RepoUrl,
    token: SecretString,
    forge: Option<GitForgeKind>,
    operation: &str,
) -> anyhow::Result<GitForge> {
    let forge = match forge {
        Some(forge) => forge,
        None if repo.is_on_github_dot_com() => GitForgeKind::Github,
        None => anyhow::bail!(
            "Can't {operation}: the repository host isn't recognized as GitHub.com. Select a forge with `--forge`; GitHub Enterprise Server requires `--forge github`."
        ),
    };

    Ok(match forge {
        GitForgeKind::Github => GitForge::Github(GitHub::from_repo_url(repo, token)?),
        GitForgeKind::Gitea => GitForge::Gitea(Gitea::new(repo, token)?),
        GitForgeKind::Gitlab => GitForge::Gitlab(GitLab::new(repo, token)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_github_forge_rejects_unknown_host() {
        let repo = RepoUrl::new("https://git.company.example/owner/repo").unwrap();

        let error = git_forge(repo, SecretString::from("token"), None, "create PR").unwrap_err();

        assert!(error.to_string().contains("Can't create PR"));
        assert!(error.to_string().contains("--forge github"));
    }

    #[test]
    fn explicit_github_forge_accepts_enterprise_host() {
        let repo = RepoUrl::new("https://git.company.example/owner/repo").unwrap();

        assert!(matches!(
            git_forge(
                repo,
                SecretString::from("token"),
                Some(GitForgeKind::Github),
                "create release"
            )
            .unwrap(),
            GitForge::Github(_)
        ));
    }
}
