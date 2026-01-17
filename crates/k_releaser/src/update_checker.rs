use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ClientResponse {
    pub tag_name: String,
}

pub async fn check_update() -> anyhow::Result<()> {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    let latest_version = get_latest_version()
        .await
        .context("error while checking for updates")?;
    if latest_version == CURRENT_VERSION {
        println!("Your release-plz version ({CURRENT_VERSION}) is up to date");
    } else {
        println!(
            "Your release-plz version is {CURRENT_VERSION}. A newer version ({latest_version}) is available at https://github.com/release-plz/release-plz"
        );
    }
    Ok(())
}

async fn get_latest_version() -> anyhow::Result<String> {
    let client = k_releaser_core::http_client::http_client_builder()
        .build()
        .context("can't build GitHub client")?;
    let client_response = client
        .get("https://api.github.com/repos/release-plz/release-plz/releases/latest")
        .send()
        .await
        .context("error while sending request")?;

    let tag_name = client_response
        .json::<ClientResponse>()
        .await
        .ok()
        .context("can't parse response")?
        .tag_name;

    extract_version(&tag_name)
        .with_context(|| {
            format!("can't extract latest release-plz version from tag name {tag_name}")
        })
        .map(|v| v.to_string())
}

fn extract_version(tag: &str) -> Option<&str> {
    tag.strip_prefix("release-plz-v")
}

#[cfg(test)]
mod tests {
    use super::extract_version;

    #[test]
    fn version_is_extracted() {
        let tag = "release-plz-v0.2.37";
        assert_eq!(extract_version(tag), Some("0.2.37"));
    }
}
