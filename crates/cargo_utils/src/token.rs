use std::{collections::HashMap, path::PathBuf};

use anyhow::Context as _;
use secrecy::SecretString;
use serde::Deserialize;

pub fn registry_token(registry: Option<&str>) -> anyhow::Result<Option<SecretString>> {
    let mut token = registry_token_from_env(registry)?;
    if token.is_none() {
        token = registry_token_from_credential_file(registry).with_context(|| {
            format!(
                "can't retrieve token from credential file for registry `{}`",
                registry.unwrap_or("crates.io"),
            )
        })?;
    }
    Ok(token)
}

/// Read credentials for a specific registry using environment variables.
/// <https://doc.rust-lang.org/cargo/reference/registry-authentication.html#cargotoken>
fn registry_token_from_env(registry: Option<&str>) -> anyhow::Result<Option<SecretString>> {
    let env_var_name = crate::cargo_registries_token_env_var_name(registry.unwrap_or("crates-io"))?;
    Ok(std::env::var(env_var_name).ok().map(SecretString::from))
}

/// Read credentials for a specific registry using file cargo/credentials.toml.
/// <https://doc.rust-lang.org/cargo/reference/config.html#credentials>
fn registry_token_from_credential_file(
    registry: Option<&str>,
) -> anyhow::Result<Option<SecretString>> {
    let credentials = read_cargo_credentials()?;
    let token = credentials
        .as_ref()
        .and_then(|c| c.token(registry))
        .map(SecretString::from);
    Ok(token)
}

fn read_cargo_credentials() -> anyhow::Result<Option<CargoCredentials>> {
    credentials_path()?
        .map(|credentials_path| {
            let content = fs_err::read_to_string(&credentials_path)
                .context("failed to read cargo credentials file")?;
            toml::from_str::<CargoCredentials>(&content).context("Invalid cargo credentials file")
        })
        .transpose()
}

fn credentials_path() -> anyhow::Result<Option<PathBuf>> {
    let cargo_home = cargo_home()?;
    let mut path = cargo_home.join("credentials.toml");
    if !path.exists() {
        path = cargo_home.join("credentials");
    }
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(path))
}

fn cargo_home() -> anyhow::Result<PathBuf> {
    let default_cargo_home = std::env::home_dir()
        .map(|x| x.join(".cargo"))
        .context("Failed to read home directory")?;
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or(default_cargo_home);
    Ok(cargo_home)
}

#[derive(Debug, Deserialize)]
struct CargoCredentials {
    #[serde(default)]
    registry: Option<RegistryToken>,
    #[serde(default)]
    registries: HashMap<String, RegistryToken>,
}

impl CargoCredentials {
    /// Token of the given `registry` ([`Option::None`] means crates.io).
    fn token(&self, registry: Option<&str>) -> Option<&str> {
        let registry_token = match registry {
            // `cargo login` stores the crates.io token in `[registry]`.
            None | Some("crates-io") => self.registry.as_ref(),
            Some(r) => self.registries.get(r),
        };
        registry_token.and_then(|t| t.token.as_deref())
    }
}

#[derive(Debug, Deserialize)]
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
        assert_eq!(creds.token(None), Some("aaaa"));
        // Packages with `publish = ["crates-io"]` refer to crates.io by name,
        // but its token is in `[registry]`, not in `[registries.crates-io]`.
        assert_eq!(creds.token(Some("crates-io")), Some("aaaa"));
        assert_eq!(creds.token(Some("my")), Some("bbb"));
        assert_eq!(creds.token(Some("foo")), None);
    }

    #[test]
    fn test_parse_cargo_credentials_cratesio_only() {
        let sample = r#"
            [registry]
            token = "aaaa"   # Access token for crates.io
        "#;
        let creds = toml::from_str::<CargoCredentials>(sample).unwrap();
        assert_eq!(creds.token(None), Some("aaaa"));
        assert_eq!(creds.token(Some("my")), None);
        assert_eq!(creds.token(Some("foo")), None);
    }

    #[test]
    fn test_parse_cargo_credentials_empty() {
        let sample = "";
        let creds = toml::from_str::<CargoCredentials>(sample).unwrap();
        assert_eq!(creds.token(None), None);
        assert_eq!(creds.token(Some("my")), None);
        assert_eq!(creds.token(Some("foo")), None);
    }
}
