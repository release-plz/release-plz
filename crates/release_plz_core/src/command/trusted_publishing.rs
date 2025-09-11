use anyhow::Context;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use tracing::info;

const CRATES_IO_BASE_URL: &str = "https://crates.io";
// Public API used by release logic
pub(crate) async fn get_crates_io_token() -> anyhow::Result<String> {
    let audience = audience_from_url(CRATES_IO_BASE_URL);
    info!("Retrieving GitHub Actions JWT token with audience: {audience}");
    let jwt = get_github_actions_jwt(&audience).await?;
    info!("Retrieved JWT token successfully");
    info!(
        "Requesting token from: {}",
        get_tokens_endpoint(CRATES_IO_BASE_URL)
    );
    let token = request_trusted_publishing_token(CRATES_IO_BASE_URL, &jwt).await?;
    info!("Retrieved token successfully");
    Ok(token)
}

pub(crate) async fn revoke_crates_io_token(token: &str) -> anyhow::Result<()> {
    info!(
        "Revoking trusted publishing token at {}",
        get_tokens_endpoint(CRATES_IO_BASE_URL)
    );
    revoke_trusted_publishing_token(CRATES_IO_BASE_URL, token).await?;
    info!("Token revoked successfully");
    Ok(())
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

fn user_agent_value() -> String {
    // Identify release-plz to crates.io
    format!("release-plz/{}", env!("CARGO_PKG_VERSION"))
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

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {req_token}"))?,
    );
    headers.insert(USER_AGENT, HeaderValue::from_str(&user_agent_value())?);

    let client = reqwest::Client::new();
    let resp = client.get(full_url).headers(headers).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to get GitHub Actions OIDC token. Status: {status}. Body: {body}");
    }
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
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_str(&user_agent_value())?);
    let client = reqwest::Client::new();
    let resp = client
        .post(endpoint)
        .headers(headers)
        .body(serde_json::to_vec(&serde_json::json!({"jwt": jwt}))?)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Failed to retrieve token from Cargo registry. Status: {status}. Response: {text}"
        );
    }
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

async fn revoke_trusted_publishing_token(
    registry_base_url: &str,
    token: &str,
) -> anyhow::Result<()> {
    let endpoint = get_tokens_endpoint(registry_base_url);
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&user_agent_value())?);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    let client = reqwest::Client::new();
    let resp = client.delete(endpoint).headers(headers).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Failed to revoke trusted publishing token. Status: {}. Response: {}",
            status,
            text
        );
    }
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
