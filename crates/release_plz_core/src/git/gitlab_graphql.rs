//! Helpers for the GitLab [GraphQL API](https://docs.gitlab.com/api/graphql/).

use anyhow::Context as _;
use serde::Deserialize;
use serde_json::json;
use tracing::debug;
use url::Url;

use crate::GitClient;
use crate::response_ext::ResponseExt;

/// Query the GitLab user that authored a commit.
///
/// GitLab resolves the author of the commit to a user account by email.
/// The `author` field is `null` when no account matches the email.
/// See <https://docs.gitlab.com/api/graphql/reference/#commit>.
const COMMIT_AUTHOR_QUERY: &str = "\
query($fullPath: ID!, $ref: String!) {
  project(fullPath: $fullPath) {
    repository {
      commit(ref: $ref) {
        author {
          username
        }
      }
    }
  }
}";

#[derive(Deserialize, Debug)]
struct CommitAuthorResponse {
    data: Option<CommitAuthorData>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct CommitAuthorData {
    project: Option<Project>,
}

#[derive(Deserialize, Debug)]
struct Project {
    repository: Option<Repository>,
}

#[derive(Deserialize, Debug)]
struct Repository {
    commit: Option<Commit>,
}

#[derive(Deserialize, Debug)]
struct Commit {
    author: Option<Author>,
}

#[derive(Deserialize, Debug)]
struct Author {
    username: String,
}

/// Username of the GitLab user that authored the given commit.
///
/// Returns `None` if the commit isn't in the remote repository
/// or if no GitLab user matches the email of the commit author.
pub async fn commit_author_username(
    client: &GitClient,
    commit: &str,
) -> anyhow::Result<Option<String>> {
    let full_path = client.remote.owner_slash_repo();
    let endpoint = graphql_endpoint_from_rest_base(&client.remote.base_url);
    debug!("Looking up the author of commit {commit} with the GitLab GraphQL API at {endpoint}");

    // The `PRIVATE-TOKEN` header set by the client also authenticates GraphQL requests.
    let response: CommitAuthorResponse = client
        .client
        .post(endpoint)
        .json(&json!({
            "query": COMMIT_AUTHOR_QUERY,
            "variables": { "fullPath": full_path, "ref": commit },
        }))
        .send()
        .await?
        .successful_status()
        .await
        .with_context(|| format!("can't query the author of commit {commit}"))?
        .json()
        .await
        .context("can't parse GitLab GraphQL response")?;

    if !response.errors.is_empty() {
        anyhow::bail!(
            "GitLab GraphQL commit query returned errors: {}",
            serde_json::to_string(&response.errors)?
        );
    }

    let project = response
        .data
        .and_then(|data| data.project)
        .with_context(|| {
            format!(
                "GitLab project `{full_path}` not found. Make sure the token has access to the project."
            )
        })?;

    let repository = project.repository.with_context(|| {
        format!(
            "can't read the repository of GitLab project `{full_path}`. Make sure the token has permission to read the repository."
        )
    })?;

    let Some(commit_info) = repository.commit else {
        debug!("Commit {commit} not found in the remote repository");
        return Ok(None);
    };

    let username = commit_info.author.map(|author| author.username);
    if username.is_none() {
        debug!("No GitLab user matches the author email of commit {commit}");
    }
    Ok(username)
}

/// GitLab serves the GraphQL API at `/api/graphql` on the same host as the REST API.
fn graphql_endpoint_from_rest_base(rest_base: &Url) -> Url {
    let mut graphql = rest_base.clone();
    graphql.set_path("/api/graphql");
    graphql
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    use super::*;
    use crate::{GitForge, GitLab, RepoUrl};

    fn gitlab_client(server: &MockServer) -> GitClient {
        let repo_url = RepoUrl::new(&format!("{}/group/subgroup/repo", server.uri())).unwrap();
        let gitlab = GitLab::new(repo_url, SecretString::from("token")).unwrap();
        GitClient::new(GitForge::Gitlab(gitlab)).unwrap()
    }

    async fn mock_commit_author_query(server: &MockServer, response: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(header("PRIVATE-TOKEN", "token"))
            .and(body_json(json!({
                "query": COMMIT_AUTHOR_QUERY,
                "variables": { "fullPath": "group/subgroup/repo", "ref": "abc" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(server)
            .await;
    }

    fn commit_response(commit: &serde_json::Value) -> serde_json::Value {
        json!({ "data": { "project": { "repository": { "commit": commit } } } })
    }

    #[tokio::test]
    async fn username_is_read_from_commit_author() {
        let server = MockServer::start().await;
        mock_commit_author_query(
            &server,
            commit_response(&json!({ "author": { "username": "bob" } })),
        )
        .await;
        // Go through `GitClient` so the GitLab arm of `get_remote_commit` is covered too.
        let remote_commit = gitlab_client(&server)
            .get_remote_commit("abc")
            .await
            .unwrap();
        assert_eq!(remote_commit.username.as_deref(), Some("bob"));
    }

    #[tokio::test]
    async fn username_is_none_when_no_user_matches_author_email() {
        let server = MockServer::start().await;
        mock_commit_author_query(&server, commit_response(&json!({ "author": null }))).await;
        let username = commit_author_username(&gitlab_client(&server), "abc")
            .await
            .unwrap();
        assert_eq!(username, None);
    }

    #[tokio::test]
    async fn username_is_none_when_commit_is_not_in_remote_repository() {
        let server = MockServer::start().await;
        mock_commit_author_query(&server, commit_response(&json!(null))).await;
        let username = commit_author_username(&gitlab_client(&server), "abc")
            .await
            .unwrap();
        assert_eq!(username, None);
    }

    #[tokio::test]
    async fn missing_project_is_an_error() {
        let server = MockServer::start().await;
        mock_commit_author_query(&server, json!({ "data": { "project": null } })).await;
        let error = commit_author_username(&gitlab_client(&server), "abc")
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("`group/subgroup/repo` not found"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn unreadable_repository_is_an_error() {
        let server = MockServer::start().await;
        let response = json!({ "data": { "project": { "repository": null } } });
        mock_commit_author_query(&server, response).await;
        let error = commit_author_username(&gitlab_client(&server), "abc")
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("can't read the repository of GitLab project `group/subgroup/repo`"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn graphql_errors_are_reported() {
        let server = MockServer::start().await;
        let response = json!({ "data": null, "errors": [{ "message": "query is too complex" }] });
        mock_commit_author_query(&server, response).await;
        let error = commit_author_username(&gitlab_client(&server), "abc")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("query is too complex"),
            "{error}"
        );
    }

    #[test]
    fn graphql_endpoint_is_derived_from_rest_base() {
        for (rest, graphql) in [
            (
                "https://gitlab.com/api/v4/projects/group%2Frepo",
                "https://gitlab.com/api/graphql",
            ),
            (
                "https://gitlab.example.com:8443/api/v4/projects/group%2Fsubgroup%2Frepo",
                "https://gitlab.example.com:8443/api/graphql",
            ),
        ] {
            let rest: Url = rest.parse().unwrap();
            assert_eq!(graphql_endpoint_from_rest_base(&rest).as_str(), graphql);
        }
    }
}
