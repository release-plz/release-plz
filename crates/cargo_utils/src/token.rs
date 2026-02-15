use std::{collections::HashMap, path::PathBuf};

use anyhow::Context as _;
use secrecy::SecretString;
use serde::Deserialize;

pub fn registry_token(registry: Option<&str>) -> anyhow::Result<Option<SecretString>> {
    let mut token = registry_token_from_env(registry)?;
    if token.is_none() {
        token = registry_token_from_credential_file(registry).with_context(|| {
            format!(
                "can't retreive token from credential file for registry `{}`",
                registry.unwrap_or("crates.io"),
            )
        })?;
    }
    Ok(token)
}

/// Read credentials for a specific registry using environment variables.
/// <https://doc.rust-lang.org/cargo/reference/registry-authentication.html#cargotoken>
pub fn registry_token_from_env(registry: Option<&str>) -> anyhow::Result<Option<SecretString>> {
    let token = if let Some(r) = registry {
        let env_var_name = crate::cargo_registries_token_env_var_name(r)?;
        std::env::var(env_var_name)
    } else {
        std::env::var("CARGO_REGISTRY_TOKEN")
    };
    Ok(token.ok().map(|t| t.into()))
}

/// Read credentials for a specific registry using file cargo/credentials.toml.
/// <https://doc.rust-lang.org/cargo/reference/config.html#credentials>
pub fn registry_token_from_credential_file(
    registry: Option<&str>,
) -> anyhow::Result<Option<SecretString>> {
    let credentials = read_cargo_credentials()?;
    let token = credentials
        .and_then(|c| {
            let token: Option<RegistryToken> = if let Some(r) = registry {
                c.registries.get(r).cloned()
            } else {
                c.registry.as_ref().cloned()
            };
            token
        })
        .and_then(|r| r.token.clone())
        .map(|t| t.into());
    Ok(token)
}

fn read_cargo_credentials() -> anyhow::Result<Option<CargoCredentials>> {
    let credentials_path = credentials_path()?;
    let credentials = if let Some(credentials_path) = credentials_path {
        let content = fs_err::read_to_string(&credentials_path)
            .context("failed to read cargo credentials file")?;
        let credentials = toml::from_str::<CargoCredentials>(&content)
            .context("Invalid cargo credentials file")?;
        Some(credentials)
    } else {
        None
    };
    Ok(credentials)
}

fn credentials_path() -> anyhow::Result<Option<PathBuf>> {
    let cargo_home = crate::cargo_home()?;
    let mut path = cargo_home.join("credentials.toml");
    if !path.exists() {
        path = cargo_home.join("credentials");
    }
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(path))
}

#[derive(Debug, Deserialize, Default, PartialEq)]
struct CargoCredentials {
    #[serde(default)]
    registry: Option<RegistryToken>,
    #[serde(default)]
    registries: HashMap<String, RegistryToken>,
}

#[derive(Debug, Deserialize, Default, PartialEq, Clone)]
struct RegistryToken {
    token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_credentials_both() {
        let sample = r#"
            [registry]
            token = "aaaa"   # Access token for crates.io

            [registries.my]
            token = "bbb"   # Access token for the named registry
        "#;
        let creds = toml::from_str::<CargoCredentials>(sample).unwrap();
        assert_eq!(
            creds.registry.and_then(|r| r.token),
            Some("aaaa".to_string())
        );
        assert_eq!(
            creds.registries.get("my").and_then(|r| r.token.clone()),
            Some("bbb".to_string())
        );
        assert_eq!(
            creds.registries.get("foo").and_then(|r| r.token.clone()),
            None
        );
    }

    #[test]
    fn test_parse_cargo_credentials_cratesio_only() {
        let sample = r#"
            [registry]
            token = "aaaa"   # Access token for crates.io
        "#;
        let creds = toml::from_str::<CargoCredentials>(sample).unwrap();
        assert_eq!(
            creds.registry.and_then(|r| r.token),
            Some("aaaa".to_string())
        );
        assert_eq!(
            creds.registries.get("my").and_then(|r| r.token.clone()),
            None
        );
        assert_eq!(
            creds.registries.get("foo").and_then(|r| r.token.clone()),
            None
        );
    }

    #[test]
    fn test_parse_cargo_credentials_empty() {
        let sample = "";
        let creds = toml::from_str::<CargoCredentials>(sample).unwrap();
        assert_eq!(creds.registry.and_then(|r| r.token), None);
        assert_eq!(
            creds.registries.get("my").and_then(|r| r.token.clone()),
            None
        );
        assert_eq!(
            creds.registries.get("foo").and_then(|r| r.token.clone()),
            None
        );
    }
}
