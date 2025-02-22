use anyhow::{Context, Result};
use base64::prelude::*;
use cargo_metadata::camino::Utf8PathBuf;
use git_cmd::Repo;
use serde_json::{Value, json};
use tracing::{debug, trace};
use url::Url;

use crate::GitClient;
use crate::git::backend::Remote;

/// Commit all the changes (except typestates) that are present in the repository
/// using GitHub's [GraphQL api](https://docs.github.com/en/graphql/reference/mutations#createcommitonbranch).
/// We use this API, because it gives the "Verified" status to the commit without a GPG key.
pub async fn commit_changes(
    client: &GitClient,
    repo: &Repo,
    message: &str,
    branch: &str,
) -> Result<()> {
    let commit = GithubCommit::new(&client.remote.owner_slash_repo(), repo, message, branch)?;
    let graphql_endpoint = get_graphql_endpoint(&client.remote);

    let commit_query = commit
        .to_query_json()
        .await
        .context("failed to build GitHub commit query")?;
    debug!("Sending createCommitOnBranch to {}", graphql_endpoint);
    trace!("{}", commit_query);

    let res: Value = client
        .client
        .post(graphql_endpoint)
        .json(&commit_query)
        .send()
        .await?
        .json()
        .await?;

    if let Some(errors) = res.get("errors").and_then(Value::as_array) {
        anyhow::bail!(
            "createCommitOnBranch returned errors: {:?}",
            serde_json::to_string(errors)?
        );
    }

    Ok(())
}

fn get_graphql_endpoint(remote: &Remote) -> Url {
    let mut base_url = remote.base_url.clone();
    base_url.set_path("graphql");

    base_url
}

// get the list of changes in repository excluding typechanges and removed files
fn changed_files(repo: &Repo) -> Result<Vec<String>> {
    repo.changes(|line| !line.starts_with("T ") && !line.starts_with("D "))
}

// get the list of removed files in repository
fn removed_files(repo: &Repo) -> Result<Vec<String>> {
    repo.changes(|line| line.starts_with("D "))
}

struct GithubCommit {
    owner_slash_repo: String,
    branch: String,
    message: String,
    current_head: String,
    deletions: Vec<String>,
    additions: Vec<String>,
    repo_dir: Utf8PathBuf,
}

impl GithubCommit {
    fn new(owner_slash_repo: &str, repo: &Repo, message: &str, branch: &str) -> Result<Self> {
        Ok(Self {
            owner_slash_repo: owner_slash_repo.to_owned(),
            branch: branch.to_owned(),
            message: message.to_owned(),
            current_head: repo.current_commit_hash()?,
            deletions: removed_files(repo)?,
            additions: changed_files(repo)?,
            repo_dir: repo.directory().to_owned(),
        })
    }

    // format a graphql query json payload to create commit on branch
    async fn to_query_json(&self) -> Result<serde_json::Value> {
        let GithubCommit {
            owner_slash_repo,
            branch,
            message,
            current_head,
            ..
        } = self;

        let deletions = self
            .deletions
            .iter()
            .map(|path| json!({"path": path}))
            .collect::<Vec<_>>();

        let additions = self
            .get_additions()
            .await
            .context("failed to get git additions")?;

        let input = json!({
            "branch": {
                "repositoryNameWithOwner": owner_slash_repo,
                "branchName": branch,
            },
            "message": {"headline": message},
            "expectedHeadOid": current_head,
            "fileChanges": {
                "deletions": deletions,
                "additions": additions
            }
        });

        Ok(json!({"query": mutation(), "variables": {"input": input}}))
    }

    async fn get_additions(&self) -> anyhow::Result<Vec<Value>> {
        let mut additions = vec![];
        for path in &self.additions {
            let realpath = self.repo_dir.join(path);
            // In https://github.com/release-plz/release-plz/issues/1360#issuecomment-2095871195
            // there was a case where an addition was a directory.
            // In that case, the directory was a git submodule.
            if realpath.is_dir() {
                debug!("skipping directory `{realpath}` in git additions");
            } else {
                let realpath_content = fs_err::tokio::read(realpath).await?;
                let contents = BASE64_STANDARD.encode(realpath_content);
                additions.push(json!({"path": path, "contents": contents}));
            }
        }
        Ok(additions)
    }
}

fn mutation() -> String {
    const MUTATION: &str = r#"
            mutation($input: CreateCommitOnBranchInput!) {
              createCommitOnBranch(input: $input) {
                commit {
                  author {
                    name,
                    email
                  }
                }
              }
            }"#;

    MUTATION.replace(|c: char| c.is_whitespace(), "")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    use crate::copy_dir::create_symlink;

    #[tokio::test]
    async fn github_commit_query() {
        let temporary = tempdir().unwrap();
        let repo_dir = temporary.as_ref();
        let repo = Repo::init(repo_dir);

        // make the initial commit on top which we'll make changes
        let unchanged_path = repo_dir.join("unchanged.txt");
        fs_err::tokio::write(&unchanged_path, b"unchanged")
            .await
            .unwrap();

        let changed = "changed.txt";
        let changed_path = repo_dir.join(changed);
        fs_err::tokio::write(&changed_path, b"changed")
            .await
            .unwrap();

        let removed = "removed.txt";
        let removed_path = repo_dir.join(removed);
        fs_err::tokio::write(&removed_path, b"removed")
            .await
            .unwrap();

        let type_changed_path = repo_dir.join("type_changed.txt");
        create_symlink(&unchanged_path, &type_changed_path).unwrap();

        repo.add_all_and_commit("initial commit").unwrap();

        // apply changes to the repository

        // file addition
        let added = "added.txt";
        let added_path = repo_dir.join(added);
        let added_base64_content = BASE64_STANDARD.encode(b"added");
        fs_err::tokio::write(&added_path, b"added").await.unwrap();

        // file change
        let changed_base64_content = BASE64_STANDARD.encode(b"file changed");
        fs_err::tokio::write(&changed_path, b"file changed")
            .await
            .unwrap();

        // file removal
        fs_err::tokio::remove_file(&removed_path).await.unwrap();

        // type change (replace symlink with a content it pointed to)
        fs_err::tokio::remove_file(&type_changed_path)
            .await
            .unwrap();
        fs_err::tokio::write(&type_changed_path, b"unchanged")
            .await
            .unwrap();

        // check if the commit query is correctly created
        let owner_slash_repo = "owner/repo";
        let branch = "main";
        let message = "message";
        let current_head = repo.current_commit_hash().unwrap();

        let expected_variables = json!({
            "input": {
                "branch": {
                    "repositoryNameWithOwner": owner_slash_repo,
                    "branchName": branch,
                },
                "message": {"headline": message},
                "expectedHeadOid": current_head,
                "fileChanges": {
                    "deletions": [{"path": removed}],
                    "additions": [
                        {"path": changed, "contents": changed_base64_content},
                        {"path": added, "contents": added_base64_content},
                    ]
                }
            }
        });

        let query = GithubCommit::new(owner_slash_repo, &repo, message, branch)
            .unwrap()
            .to_query_json()
            .await
            .unwrap();

        assert_eq!(expected_variables, query["variables"]);

        expect_test::expect![[r#""mutation($input:CreateCommitOnBranchInput!){createCommitOnBranch(input:$input){commit{author{name,email}}}}""#]]
        .assert_eq(&query["query"].to_string());
    }
}
