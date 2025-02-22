use crate::git::{gitea_client::Gitea, gitlab_client::GitLab};
use crate::{GitHub, GitReleaseInfo};
use std::collections::{HashMap, HashSet};

use crate::pr::Pr;
use anyhow::Context;
use http::StatusCode;
use itertools::Itertools;
use reqwest::header::HeaderMap;
use reqwest::{Response, Url};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, instrument};

#[derive(Debug, Clone)]
pub enum GitBackend {
    Github(GitHub),
    Gitea(Gitea),
    Gitlab(GitLab),
}

impl GitBackend {
    fn default_headers(&self) -> anyhow::Result<HeaderMap> {
        match self {
            GitBackend::Github(g) => g.default_headers(),
            GitBackend::Gitea(g) => g.default_headers(),
            GitBackend::Gitlab(g) => g.default_headers(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendType {
    Github,
    Gitea,
    Gitlab,
}

#[derive(Debug)]
pub struct GitClient {
    pub backend: BackendType,
    pub remote: Remote,
    pub client: reqwest_middleware::ClientWithMiddleware,
}

#[derive(Debug, Clone)]
pub struct Remote {
    pub owner: String,
    pub repo: String,
    pub token: SecretString,
    pub base_url: Url,
}

impl Remote {
    pub fn owner_slash_repo(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

#[derive(Deserialize)]
pub struct PrCommit {
    pub author: Option<Author>,
    pub sha: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Author {
    pub login: String,
}

// https://docs.gitlab.com/ee/api/merge_requests.html#get-single-merge-request-commits
#[derive(Deserialize, Clone, Debug)]
pub struct GitLabMrCommit {
    pub id: String,
}

impl From<GitLabMrCommit> for PrCommit {
    fn from(value: GitLabMrCommit) -> Self {
        PrCommit {
            author: None,
            sha: value.id,
        }
    }
}

#[derive(Serialize)]
pub struct CreateReleaseOption<'a> {
    tag_name: &'a str,
    body: &'a str,
    name: &'a str,
    draft: &'a bool,
    prerelease: &'a bool,
    /// Only supported by GitHub.
    #[serde(skip_serializing_if = "Option::is_none")]
    make_latest: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GitPr {
    pub user: Author,
    pub number: u64,
    pub html_url: Url,
    pub head: Commit,
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<Label>,
}

/// Pull request.
impl GitPr {
    pub fn branch(&self) -> &str {
        self.head.ref_field.as_str()
    }

    pub fn label_names(&self) -> Vec<&str> {
        self.labels.iter().map(|l| l.name.as_str()).collect()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Label {
    pub name: String,
    /// ID of the label.
    /// Used by Gitea and GitHub. Not present in GitLab responses.
    id: Option<u64>,
}

impl From<GitLabMr> for GitPr {
    fn from(value: GitLabMr) -> Self {
        let body = if value.description.is_empty() {
            None
        } else {
            Some(value.description)
        };

        let labels = value
            .labels
            .into_iter()
            .map(|l| Label { name: l, id: None })
            .collect();

        GitPr {
            number: value.iid,
            html_url: value.web_url,
            head: Commit {
                ref_field: value.source_branch,
                sha: value.sha,
            },
            title: value.title,
            body,
            user: Author {
                login: value.author.username,
            },
            labels,
        }
    }
}

/// Merge request.
#[derive(Deserialize, Clone, Debug)]
pub struct GitLabMr {
    pub author: GitLabAuthor,
    pub iid: u64,
    pub web_url: Url,
    pub sha: String,
    pub source_branch: String,
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GitLabAuthor {
    pub username: String,
}

impl From<GitPr> for GitLabMr {
    fn from(value: GitPr) -> Self {
        let desc = value.body.unwrap_or_default();
        let labels: Vec<String> = value.labels.into_iter().map(|l| l.name).collect();

        GitLabMr {
            author: GitLabAuthor {
                username: value.user.login,
            },
            iid: value.number,
            web_url: value.html_url,
            sha: value.head.sha,
            source_branch: value.head.ref_field,
            title: value.title,
            description: desc,
            labels,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Commit {
    #[serde(rename = "ref")]
    pub ref_field: String,
    pub sha: String,
}

/// Representation of a remote contributor.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct RemoteCommit {
    /// Username of the author.
    pub username: Option<String>,
}

#[derive(Serialize, Default)]
pub struct GitLabMrEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_event: Option<String>,
}

#[derive(Serialize, Default)]
pub struct PrEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
}

impl From<PrEdit> for GitLabMrEdit {
    fn from(value: PrEdit) -> Self {
        GitLabMrEdit {
            title: value.title,
            description: value.body,
            state_event: value.state,
        }
    }
}

impl PrEdit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    pub fn contains_edit(&self) -> bool {
        self.title.is_some() || self.body.is_some() || self.state.is_some()
    }
}

impl GitClient {
    pub fn new(backend: GitBackend) -> anyhow::Result<Self> {
        let client = {
            let headers = backend.default_headers()?;
            let reqwest_client = reqwest::Client::builder()
                .user_agent("release-plz")
                .default_headers(headers)
                .build()
                .context("can't build Git client")?;

            let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
            ClientBuilder::new(reqwest_client)
                // Retry failed requests.
                .with(RetryTransientMiddleware::new_with_policy(retry_policy))
                .build()
        };

        let (backend, remote) = match backend {
            GitBackend::Github(g) => (BackendType::Github, g.remote),
            GitBackend::Gitea(g) => (BackendType::Gitea, g.remote),
            GitBackend::Gitlab(g) => (BackendType::Gitlab, g.remote),
        };
        Ok(Self {
            remote,
            backend,
            client,
        })
    }

    pub fn per_page(&self) -> &str {
        match self.backend {
            BackendType::Github | BackendType::Gitlab => "per_page",
            BackendType::Gitea => "limit",
        }
    }

    /// Creates a GitHub/Gitea release.
    pub async fn create_release(&self, release_info: &GitReleaseInfo) -> anyhow::Result<()> {
        match self.backend {
            BackendType::Github | BackendType::Gitea => {
                self.create_github_release(release_info).await
            }
            BackendType::Gitlab => self.create_gitlab_release(release_info).await,
        }
        .context("Failed to create release")
    }

    /// Same as Gitea.
    pub async fn create_github_release(&self, release_info: &GitReleaseInfo) -> anyhow::Result<()> {
        if release_info.latest.is_some() && self.backend == BackendType::Gitea {
            anyhow::bail!("Gitea does not support the `git_release_latest` option");
        }
        let create_release_options = CreateReleaseOption {
            tag_name: &release_info.git_tag,
            body: &release_info.release_body,
            name: &release_info.release_name,
            draft: &release_info.draft,
            prerelease: &release_info.pre_release,
            make_latest: release_info.latest.map(|l| l.to_string()),
        };
        self.client
            .post(format!("{}/releases", self.repo_url()))
            .json(&create_release_options)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| {
                if let Some(status) = e.status() {
                    if status == reqwest::StatusCode::FORBIDDEN {
                        return anyhow::anyhow!(e).context(
                            "Make sure your token has sufficient permissions. Learn more at https://release-plz.dev/docs/usage/release or https://release-plz.dev/docs/github/token",
                        );
                    }
                }
                anyhow::anyhow!(e)
            })?;
        Ok(())
    }

    pub async fn create_gitlab_release(&self, release_info: &GitReleaseInfo) -> anyhow::Result<()> {
        #[derive(Serialize)]
        pub struct GitlabReleaseOption<'a> {
            name: &'a str,
            tag_name: &'a str,
            description: &'a str,
        }
        let gitlab_release_options = GitlabReleaseOption {
            name: &release_info.release_name,
            tag_name: &release_info.git_tag,
            description: &release_info.release_body,
        };
        self.client
            .post(format!("{}/releases", self.remote.base_url))
            .json(&gitlab_release_options)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| {
                if let Some(status) = e.status() {
                    if status == reqwest::StatusCode::FORBIDDEN {
                        return anyhow::anyhow!(e).context(
                            "Make sure your token has sufficient permissions. Learn more at https://release-plz.dev/docs/usage/release#gitlab",
                        );
                    }
                }
                anyhow::anyhow!(e)
            })?;
        Ok(())
    }

    pub fn pulls_url(&self) -> String {
        match self.backend {
            BackendType::Github | BackendType::Gitea => {
                format!("{}/pulls", self.repo_url())
            }
            BackendType::Gitlab => {
                format!("{}/merge_requests", self.repo_url())
            }
        }
    }

    pub fn issues_url(&self) -> String {
        format!("{}/issues", self.repo_url())
    }

    pub fn param_value_pr_state_open(&self) -> &'static str {
        match self.backend {
            BackendType::Github | BackendType::Gitea => "open",
            BackendType::Gitlab => "opened",
        }
    }

    fn repo_url(&self) -> String {
        match self.backend {
            BackendType::Github | BackendType::Gitea => {
                format!(
                    "{}repos/{}",
                    self.remote.base_url,
                    self.remote.owner_slash_repo()
                )
            }
            BackendType::Gitlab => self.remote.base_url.to_string(),
        }
    }

    /// Get all opened Prs which branch starts with the given `branch_prefix`.
    pub async fn opened_prs(&self, branch_prefix: &str) -> anyhow::Result<Vec<GitPr>> {
        let mut page = 1;
        let page_size = 30;
        let mut release_prs: Vec<GitPr> = vec![];
        loop {
            debug!(
                "Loading prs from {}, page {page}",
                self.remote.owner_slash_repo()
            );
            let prs: Vec<GitPr> = self
                .opened_prs_page(page, page_size)
                .await
                .context("Failed to retrieve open PRs")?;
            let prs_len = prs.len();
            let current_release_prs: Vec<GitPr> = prs
                .into_iter()
                .filter(|pr| pr.head.ref_field.starts_with(branch_prefix))
                .collect();
            release_prs.extend(current_release_prs);
            if prs_len < page_size {
                break;
            }
            page += 1;
        }
        Ok(release_prs)
    }

    async fn opened_prs_page(&self, page: i32, page_size: usize) -> anyhow::Result<Vec<GitPr>> {
        let resp = self
            .client
            .get(self.pulls_url())
            .query(&[("state", self.param_value_pr_state_open())])
            .query(&[("page", page)])
            .query(&[(self.per_page(), page_size)])
            .send()
            .await?
            .successful_status()
            .await?;

        self.prs_from_response(resp).await
    }

    async fn prs_from_response(&self, resp: Response) -> anyhow::Result<Vec<GitPr>> {
        match self.backend {
            BackendType::Github | BackendType::Gitea => {
                resp.json().await.context("failed to parse pr")
            }
            BackendType::Gitlab => {
                let gitlab_mrs: Vec<GitLabMr> =
                    resp.json().await.context("failed to parse gitlab mr")?;
                let git_prs: Vec<GitPr> = gitlab_mrs.into_iter().map(|mr| mr.into()).collect();
                Ok(git_prs)
            }
        }
    }

    async fn pr_from_response(&self, resp: Response) -> anyhow::Result<GitPr> {
        match self.backend {
            BackendType::Github | BackendType::Gitea => {
                resp.json().await.context("failed to parse pr")
            }
            BackendType::Gitlab => {
                let gitlab_mr: GitLabMr = resp.json().await.context("failed to parse gitlab mr")?;
                Ok(gitlab_mr.into())
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn close_pr(&self, pr_number: u64) -> anyhow::Result<()> {
        debug!("closing pr #{pr_number}");
        let edit = PrEdit::new().with_state(self.closed_pr_state());
        self.edit_pr(pr_number, edit)
            .await
            .with_context(|| format!("cannot close pr {pr_number}"))?;
        info!("closed pr #{pr_number}");
        Ok(())
    }

    fn closed_pr_state(&self) -> &'static str {
        match self.backend {
            BackendType::Github | BackendType::Gitea => "closed",
            BackendType::Gitlab => "close",
        }
    }

    pub async fn edit_pr(&self, pr_number: u64, pr_edit: PrEdit) -> anyhow::Result<()> {
        let req = match self.backend {
            BackendType::Github | BackendType::Gitea => self
                .client
                .patch(format!("{}/{}", self.pulls_url(), pr_number))
                .json(&pr_edit),
            BackendType::Gitlab => {
                let edit_mr: GitLabMrEdit = pr_edit.into();
                self.client
                    .put(format!("{}/merge_requests/{pr_number}", self.repo_url()))
                    .json(&edit_mr)
            }
        };
        debug!("editing pr: {req:?}");

        req.send()
            .await
            .with_context(|| format!("cannot edit pr {pr_number}"))?;

        Ok(())
    }

    #[instrument(skip(self, pr))]
    pub async fn open_pr(&self, pr: &Pr) -> anyhow::Result<GitPr> {
        debug!("Opening PR in {}", self.remote.owner_slash_repo());

        let json_body = match self.backend {
            BackendType::Github | BackendType::Gitea => json!({
                "title": pr.title,
                "body": pr.body,
                "base": pr.base_branch,
                "head": pr.branch,
                "draft": pr.draft,
            }),
            BackendType::Gitlab => json!({
                "title": pr.title,
                "description": pr.body,
                "target_branch": pr.base_branch,
                "source_branch": pr.branch,
                "draft": pr.draft,
            }),
        };

        let rep = self
            .client
            .post(self.pulls_url())
            .json(&json_body)
            .send()
            .await
            .context("failed when sending the response")?
            .successful_status()
            .await
            .context("received unexpected response")?;

        let git_pr: GitPr = match self.backend {
            BackendType::Github | BackendType::Gitea => {
                rep.json().await.context("Failed to parse PR")?
            }
            BackendType::Gitlab => {
                let gitlab_mr: GitLabMr = rep.json().await.context("Failed to parse Gitlab MR")?;
                gitlab_mr.into()
            }
        };

        info!("opened pr: {}", git_pr.html_url);
        self.add_labels(&pr.labels, git_pr.number)
            .await
            .context("Failed to add labels")?;
        Ok(git_pr)
    }

    #[instrument(skip(self))]
    pub async fn add_labels(&self, labels: &[String], pr_number: u64) -> anyhow::Result<()> {
        if labels.is_empty() {
            return Ok(());
        }

        match self.backend {
            BackendType::Github => self.post_github_labels(labels, pr_number).await,
            BackendType::Gitlab => self.post_gitlab_labels(labels, pr_number).await,
            BackendType::Gitea => self.post_gitea_labels(labels, pr_number).await,
        }
    }

    fn pr_labels_url(&self, pr_number: u64) -> String {
        format!("{}/{}/labels", self.issues_url(), pr_number)
    }

    /// Add all labels to PR
    async fn post_github_labels(&self, labels: &[String], pr_number: u64) -> anyhow::Result<()> {
        self.client
            .post(self.pr_labels_url(pr_number))
            .json(&json!({
                "labels": labels
            }))
            .send()
            .await?
            .successful_status()
            .await?;

        Ok(())
    }

    /// Add all labels to PR
    async fn post_gitlab_labels(&self, labels: &[String], pr_number: u64) -> anyhow::Result<()> {
        self.client
            .put(format!("{}/{}", self.pulls_url(), pr_number))
            .json(&json!({
                "add_labels": labels.iter().join(",")
            }))
            .send()
            .await?
            .successful_status()
            .await?;

        Ok(())
    }

    /// Add all labels to PR
    async fn post_gitea_labels(&self, labels: &[String], pr_number: u64) -> anyhow::Result<()> {
        let (labels_to_create, mut label_ids) = self
            .get_labels_info_and_categorize_labels(labels, pr_number)
            .await?;
        let new_label_ids = self.create_gitea_labels(&labels_to_create).await?;
        label_ids.extend(new_label_ids);
        anyhow::ensure!(
            !label_ids.is_empty(),
            "The provided labels: {labels:?} \n
                were not added to PR #{pr_number}",
        );
        self.client
            .post(self.pr_labels_url(pr_number))
            .json(&json!({ "labels": label_ids }))
            .send()
            .await?
            .successful_status()
            .await?;
        Ok(())
    }

    /// Get Gitea and GitHub repository labels
    async fn get_repository_labels(&self) -> anyhow::Result<Vec<Label>> {
        self.client
            .get(format!("{}/labels", self.repo_url()))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("failed to parse labels")
    }

    /// Retrieves and categorizes labels for a PR, ensuring exact matching and deduplication
    /// within the input and against existing PR labels.
    /// # Returns
    /// A tuple containing:
    /// - Vec<String>: Labels that need to be created in the repository
    /// - Vec<u64>: IDs of existing labels to be added to the PR (excluding duplicates and ones already present)
    async fn get_labels_info_and_categorize_labels(
        &self,
        labels: &[String],
        pr_number: u64,
    ) -> anyhow::Result<(Vec<String>, Vec<u64>)> {
        // Fetch both existing repository labels and current PR labels concurrently
        let (existing_labels, pr_info) =
            tokio::try_join!(self.get_repository_labels(), self.get_pr_info(pr_number))?;

        // Create map for lookups
        let existing_label_map: HashMap<&str, &Label> = existing_labels
            .iter()
            .map(|l| (l.name.as_str(), l))
            .collect();

        // Get current PR labels
        let current_pr_labels: HashSet<&str> =
            pr_info.labels.iter().map(|l| l.name.as_str()).collect();

        let mut labels_to_create: Vec<String> = vec![];
        let mut label_ids = Vec::new();

        for label in labels {
            match existing_label_map.get(label.as_str()) {
                Some(l) => {
                    // The label already exists in the repository.
                    // If the label isn't already in the PR, we add it using the label ID.
                    if !current_pr_labels.contains(label.as_str()) {
                        // The label ID is present for Gitea and GitHub
                        label_ids.push(l.id.with_context(|| {
                            format!("failed to extract id from existing label '{}'", l.name)
                        })?);
                    }
                }
                None => {
                    // The label doesn't exist in the repository, so we need to create it.
                    if !labels_to_create.contains(label) {
                        labels_to_create.push(label.to_string());
                    }
                }
            }
        }

        Ok((labels_to_create, label_ids))
    }

    async fn create_gitea_labels(&self, labels_to_create: &[String]) -> anyhow::Result<Vec<u64>> {
        let mut label_ids = Vec::new();

        for label in labels_to_create {
            let label_id = self.create_gitea_repository_label(label).await?;
            label_ids.push(label_id);
        }

        Ok(label_ids)
    }

    async fn create_gitea_repository_label(&self, label: &str) -> anyhow::Result<u64> {
        debug!("Backend Gitea creating label: {label}");
        let res = self
            .client
            .post(format!("{}/labels", self.repo_url()))
            .json(&json!({
                "name": label.trim(),
                // Required field - using white (#FFFFFF) as default color
                "color": "#FFFFFF"
            }))
            .send()
            .await?
            .error_for_status()
            .map_err(|err| {
                let status = err.status();
                let err = anyhow::anyhow!(err);
                match status {
                    Some(StatusCode::NOT_FOUND) => {
                        err.context(format!(
                        "Please check if the repository URL '{}' is correct and the user has the necessary permissions to add labels",
                        self.repo_url()
                        ))
                    }
                    Some(StatusCode::UNPROCESSABLE_ENTITY) => {
                        err.context("Please open a GitHub issue: https://github.com/release-plz/release-plz/issues")
                    }
                    _ => {
                        err.context("HTTP response contained no status code when creating label")
                    }
                }
                .context(format!("failed to create label '{label}'"))
        })?;

        let new_label: Label = res.json().await?;
        let label_id = new_label
            .id
            .with_context(|| format!("failed to extract id from label {label}"))?;
        Ok(label_id)
    }

    pub async fn pr_commits(&self, pr_number: u64) -> anyhow::Result<Vec<PrCommit>> {
        let resp = self
            .client
            .get(format!("{}/{}/commits", self.pulls_url(), pr_number))
            .send()
            .await?
            .successful_status()
            .await?;
        self.parse_pr_commits(resp).await
    }

    async fn parse_pr_commits(&self, resp: Response) -> anyhow::Result<Vec<PrCommit>> {
        match self.backend {
            BackendType::Github | BackendType::Gitea => {
                resp.json().await.context("failed to parse pr commits")
            }
            BackendType::Gitlab => {
                let gitlab_commits: Vec<GitLabMrCommit> =
                    resp.json().await.context("failed to parse gitlab mr")?;
                let pr_commits = gitlab_commits
                    .into_iter()
                    .map(|commit| commit.into())
                    .collect();
                Ok(pr_commits)
            }
        }
    }

    /// Only works for GitHub.
    /// From my tests, Gitea doesn't work yet,
    /// but this implementation should be correct.
    pub async fn associated_prs(&self, commit: &str) -> anyhow::Result<Vec<GitPr>> {
        let url = match self.backend {
            BackendType::Github => {
                format!("{}/commits/{}/pulls", self.repo_url(), commit)
            }
            BackendType::Gitea => {
                format!("{}/commits/{}/pull", self.repo_url(), commit)
            }
            BackendType::Gitlab => {
                format!(
                    "{}/repository/commits/{}/merge_requests",
                    self.repo_url(),
                    commit
                )
            }
        };

        let response = self.client.get(url).send().await?;
        if response.status() == StatusCode::NOT_FOUND
            // GitHub returns 422 if the commit doesn't exist/hasn't been pushed to the remote repository.
            || (response.status() == StatusCode::UNPROCESSABLE_ENTITY
                && self.backend == BackendType::Github)
        {
            debug!(
                "No associated PRs for commit {commit}. This can happen if the commit is not pushed to the remote repository."
            );
            return Ok(vec![]);
        }
        let response = response.error_for_status()?;
        debug!("Associated PR found. Status: {}", response.status());

        let prs = match self.backend {
            BackendType::Github => {
                let prs: Vec<GitPr> = response
                    .json()
                    .await
                    .context("can't parse associated PRs")?;
                prs
            }
            BackendType::Gitea => {
                let pr: GitPr = response.json().await.context("can't parse associated PR")?;
                vec![pr]
            }
            BackendType::Gitlab => {
                let gitlab_mrs: Vec<GitLabMr> = response
                    .json()
                    .await
                    .context("can't parse associated Gitlab MR")?;
                let git_prs: Vec<GitPr> = gitlab_mrs.into_iter().map(|mr| mr.into()).collect();
                git_prs
            }
        };

        let prs_numbers = prs.iter().map(|pr| pr.number).collect::<Vec<_>>();
        debug!("Associated PRs for commit {commit}: {:?}", prs_numbers);
        Ok(prs)
    }

    pub async fn get_pr_info(&self, pr_number: u64) -> anyhow::Result<GitPr> {
        let response = self
            .client
            .get(format!("{}/{}", self.pulls_url(), pr_number))
            .send()
            .await?
            .successful_status()
            .await?;

        self.pr_from_response(response).await
    }

    pub async fn get_prs_info(&self, pr_numbers: &[u64]) -> anyhow::Result<Vec<GitPr>> {
        let mut prs = vec![];
        for pr_number in pr_numbers {
            let pr = self.get_pr_info(*pr_number).await?;
            prs.push(pr);
        }
        Ok(prs)
    }

    pub async fn get_remote_commit(&self, commit: &str) -> Result<RemoteCommit, anyhow::Error> {
        let api_path = self.commits_api_path(commit);
        let response = self.client.get(api_path).send().await?;

        if let Err(err) = response.error_for_status_ref() {
            if let Some(StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY) = err.status() {
                // The user didn't push the commit to the remote repository.
                // This can happen if people need to do edits before running release-plz (e.g. cargo hakari).
                // I'm not sure why GitHub returns 422 if the commit doesn't exist.
                return Ok(RemoteCommit { username: None });
            }
        }

        let remote_commit: GitHubCommit = response
            .successful_status()
            .await?
            .json()
            .await
            .context("can't parse commits")?;

        let username = remote_commit.author.and_then(|author| author.login);
        Ok(RemoteCommit { username })
    }

    fn commits_api_path(&self, commit: &str) -> String {
        let commits_path = "commits/";
        let commits_api_path = match self.backend {
            BackendType::Gitea => {
                format!("git/{commits_path}")
            }
            BackendType::Github => commits_path.to_string(),
            BackendType::Gitlab => {
                unimplemented!("Gitlab support for `release-plz release-pr is not implemented yet")
            }
        };
        format!("{}/{commits_api_path}{commit}", self.repo_url())
    }
}

pub fn validate_labels(labels: &[String]) -> anyhow::Result<()> {
    let mut unique_labels: HashSet<&str> = HashSet::new();

    for l in labels {
        // use a closure to avoid allocating the error message string unless needed
        let error_msg = || format!("Failed to add label `{l}`:");

        if l.len() > 50 {
            anyhow::bail!(
                "{} it exceeds maximum length of 50 characters.",
                error_msg()
            );
        }

        if l.trim() != l {
            anyhow::bail!(
                "{} leading or trailing whitespace is not allowed.",
                error_msg()
            );
        }

        if l.is_empty() {
            anyhow::bail!("{} empty labels are not allowed.", error_msg());
        }

        let is_label_new = unique_labels.insert(l.as_str());
        if !is_label_new {
            anyhow::bail!("{} duplicate labels are not allowed.", error_msg());
        }
    }
    Ok(())
}

/// Representation of a single commit.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitHubCommit {
    /// SHA.
    pub sha: String,
    /// Author of the commit.
    pub author: Option<GitHubCommitAuthor>,
}

/// Author of the commit.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitHubCommitAuthor {
    /// Username.
    pub login: Option<String>,
}

/// Returns the list of contributors for the given commits,
/// excluding the PR author and bots.
pub fn contributors_from_commits(commits: &[PrCommit]) -> Vec<String> {
    let mut contributors = commits
        .iter()
        .skip(1) // skip pr author
        .flat_map(|commit| &commit.author)
        .filter(|author| !author.login.ends_with("[bot]")) // ignore bots
        .map(|author| author.login.clone())
        .collect::<Vec<_>>();
    contributors.dedup();
    contributors
}

trait ResponseExt {
    /// Better version of [`reqwest::Response::error_for_status`] that
    /// also captures the response body in the error message. It will most
    /// likely contain additional error details.
    async fn successful_status(self) -> anyhow::Result<reqwest::Response>;
}

impl ResponseExt for reqwest::Response {
    async fn successful_status(self) -> anyhow::Result<reqwest::Response> {
        let Err(err) = self.error_for_status_ref() else {
            return Ok(self);
        };

        let mut body = self
            .text()
            .await
            .context("can't convert response body to text")?;

        // If the response is JSON, try to pretty-print it.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            body = format!("{json:#}");
        }

        Err(err).context(format!("Response body:\n{body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contributors_are_extracted_from_commits() {
        let commits = vec![
            PrCommit {
                author: Some(Author {
                    login: "bob".to_string(),
                }),
                sha: "abc".to_string(),
            },
            PrCommit {
                author: Some(Author {
                    login: "marco".to_string(),
                }),
                sha: "abc".to_string(),
            },
            PrCommit {
                author: Some(Author {
                    login: "release[bot]".to_string(),
                }),
                sha: "abc".to_string(),
            },
            PrCommit {
                author: None,
                sha: "abc".to_string(),
            },
        ];
        let contributors = contributors_from_commits(&commits);
        assert_eq!(contributors, vec!["marco"]);
    }
}
