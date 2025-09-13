//! Docs at <https://crates.io/docs/trusted-publishing>

use anyhow::Context;
use secrecy::{ExposeSecret, SecretString};
use tracing::info;

use crate::response_ext::ResponseExt;

const CRATES_IO_BASE_URL: &str = "https://crates.io";

#[derive(Clone)]
pub struct TrustedPublisher {
    base_url: String,
    client: reqwest::Client,
    token: SecretString,
}

impl TrustedPublisher {
    /// Create a trusted publisher targeting crates.io.
    pub async fn crates_io() -> anyhow::Result<Self> {
        let client = crate::http_client::http_client_builder().build()?;
        let base_url = CRATES_IO_BASE_URL.to_string();

        // Issue a short-lived token immediately and store it in the struct
        let token = issue_token(&client, &base_url).await?;

        Ok(Self {
            base_url,
            client,
            token,
        })
    }

    fn tokens_endpoint(&self) -> String {
        get_tokens_endpoint(&self.base_url)
    }

    /// Revoke a previously issued token.
    pub async fn revoke_token(&self) -> anyhow::Result<()> {
        let endpoint = self.tokens_endpoint();
        info!("Revoking trusted publishing token at {endpoint}");
        self.client
            .delete(endpoint)
            .bearer_auth(self.token.expose_secret())
            .send()
            .await?
            .successful_status()
            .await
            .context("Failed to revoke trusted publishing token")?;
        info!("Token revoked successfully");
        Ok(())
    }

    /// Expose the retrieved token so callers can reuse it when needed (e.g., cargo publish).
    pub fn token(&self) -> &SecretString {
        &self.token
    }
}

async fn issue_token(
    client: &reqwest::Client,
    base_url: &String,
) -> Result<SecretString, anyhow::Error> {
    let audience = audience_from_url(base_url);
    info!("Retrieving GitHub Actions JWT token with audience: {audience}");
    let jwt = get_github_actions_jwt(client, &audience).await?;
    info!("Retrieved JWT token successfully");
    let token = request_trusted_publishing_token(client, base_url, &jwt).await?;
    info!("Retrieved trusted publishing token from cargo registry successfully");
    Ok(SecretString::from(token))
}
async fn get_github_actions_jwt(
    client: &reqwest::Client,
    audience: &str,
) -> anyhow::Result<String> {
    // Follow GitHub OIDC flow using environment variables provided in Actions runners
    let req_url = read_actions_id_env_var("ACTIONS_ID_TOKEN_REQUEST_URL")?;
    let req_token = read_actions_id_env_var("ACTIONS_ID_TOKEN_REQUEST_TOKEN")?;

    // Append audience query parameter
    let separator = if req_url.contains('?') { '&' } else { '?' };
    let full_url = format!(
        "{}{}audience={}",
        req_url,
        separator,
        urlencoding::encode(audience)
    );

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
    client: &reqwest::Client,
    base_url: &str,
    jwt: &str,
) -> anyhow::Result<String> {
    let endpoint = get_tokens_endpoint(base_url);
    info!("Requesting token from: {endpoint}");
    let resp = client
        .post(endpoint)
        .json(&serde_json::json!({"jwt": jwt}))
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

fn get_tokens_endpoint(registry_base_url: &str) -> String {
    let url = registry_base_url.trim_end_matches('/');
    format!("{url}/api/v1/trusted_publishing/tokens")
}

fn audience_from_url(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

fn read_actions_id_env_var(name: &str) -> anyhow::Result<String> {
    std::env::var(name)
        .with_context(|| format!("{name} not set. If you are running in GitHub Actions, please ensure the 'id-token' permission is set to 'write' in your workflow. For more information, see: https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/about-security-hardening-with-openid-connect#adding-permissions-settings"))
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
