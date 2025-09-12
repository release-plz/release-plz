use anyhow::Context;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use tracing::info;

use crate::response_ext::ResponseExt;

const CRATES_IO_BASE_URL: &str = "https://crates.io";
// Public API used by release logic
pub(crate) async fn get_crates_io_token() -> anyhow::Result<String> {
    let audience = audience_from_url(CRATES_IO_BASE_URL);
    info!("Retrieving GitHub Actions JWT token with audience: {audience}");
    let jwt = get_github_actions_jwt(&audience).await?;
    info!("Retrieved JWT token successfully");
    let token = request_trusted_publishing_token(CRATES_IO_BASE_URL, &jwt).await?;
    info!("Retrieved token successfully");
    Ok(token)
}

fn get_tokens_endpoint(registry_base_url: &str) -> String {
    let url = registry_base_url.trim_end_matches('/');
    format!("{url}/api/v1/trusted_publishing/tokens")
}

pub(crate) fn audience_from_url(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

async fn get_github_actions_jwt(audience: &str) -> anyhow::Result<String> {
    let env_request_url = "ACTIONS_ID_TOKEN_REQUEST_URL";
    // Follow GitHub OIDC flow using environment variables provided in Actions runners
    let req_url = std::env::var(env_request_url)
        .with_context(|| format!("{env_request_url} not set. If you are running in GitHub Actions,
Please ensure the 'id-token' permission is set to 'write' in your workflow. For more information, see: https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/about-security-hardening-with-openid-connect#adding-permissions-settings"))?;

    let env_request_token = "ACTIONS_ID_TOKEN_REQUEST_TOKEN";
    let req_token = std::env::var(env_request_token)
        .with_context(|| format!("{env_request_token} not set; not running in GitHub Actions?"))?;

    // Append audience query parameter
    let separator = if req_url.contains('?') { '&' } else { '?' };
    let full_url = format!(
        "{}{}audience={}",
        req_url,
        separator,
        urlencoding::encode(audience)
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(full_url)
        .bearer_auth(req_token)
        .send()
        .await?
        .successful_status()
        .await
        .context("Failed to get GitHub Actions OIDC token")?;
    #[derive(serde::Deserialize)]
    struct OidcResp {
        value: String,
    }
    let body: OidcResp = resp.json().await?;
    if body.value.is_empty() {
        anyhow::bail!("Empty OIDC token received");
    }
    Ok(body.value)
}

async fn request_trusted_publishing_token(
    registry_base_url: &str,
    jwt: &str,
) -> anyhow::Result<String> {
    let endpoint = get_tokens_endpoint(registry_base_url);
    info!("Requesting token from: {endpoint}");
    let client = crate::http_client::http_client_builder().build()?;
    let resp = client
        .post(endpoint)
        .json(&serde_json::json!([{"jwt": jwt}]))
        .send()
        .await?
        .successful_status()
        .await
        .context("Failed to retrieve token from Cargo registry")?;
    #[derive(serde::Deserialize)]
    struct TokenResp {
        token: String,
    }
    let body: TokenResp = resp.json().await?;
    if body.token.is_empty() {
        anyhow::bail!("Empty token received from registry");
    }
    Ok(body.token)
}

pub(crate) async fn revoke_crates_io_token(token: &str) -> anyhow::Result<()> {
    let endpoint = get_tokens_endpoint(CRATES_IO_BASE_URL);
    info!("Revoking trusted publishing token at {endpoint}");

    let client = crate::http_client::http_client_builder().build()?;
    client
        .delete(endpoint)
        .bearer_auth(token)
        .send()
        .await?
        .successful_status()
        .await
        .context("Failed to revoke trusted publishing token")?;
    info!("Token revoked successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn audience_from_url_works() {
        assert_eq!(super::audience_from_url("https://crates.io"), "crates.io");
        assert_eq!(super::audience_from_url("http://crates.io"), "crates.io");
        assert_eq!(super::audience_from_url("https://crates.io/"), "crates.io");
        assert_eq!(super::audience_from_url("crates.io"), "crates.io");
    }
}
