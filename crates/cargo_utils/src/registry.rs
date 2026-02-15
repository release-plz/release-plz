use anyhow::Context;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;

const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";
const CRATES_IO_REGISTRY: &str = "crates-io";

/// Read index for a specific registry using environment variables.
/// <https://doc.rust-lang.org/cargo/reference/environment-variables.html>
///
/// Returns:
/// - [`Result::Err`] if the registry name is invalid.
/// - [`Result::Ok`] with [`Option::None`] if the environment variable is not set.
pub fn registry_index_url_from_env(registry: &str) -> anyhow::Result<Option<String>> {
    let env_var_name = cargo_registries_index_env_var_name(registry)?;
    Ok(std::env::var(env_var_name).ok())
}

pub fn cargo_registries_token_env_var_name(registry: &str) -> anyhow::Result<String> {
    Ok(format!(
        "CARGO_REGISTRIES_{}_TOKEN",
        registry_env_var_name(registry)?
    ))
}

fn cargo_registries_index_env_var_name(registry: &str) -> anyhow::Result<String> {
    Ok(format!(
        "CARGO_REGISTRIES_{}_INDEX",
        registry_env_var_name(registry)?
    ))
}

/// Sanitizes the registry name to construct a valid environment variable name.
/// Mirrors Cargo's behavior:
/// - Alphanumeric characters and underscores (`_`) are preserved.
/// - Hyphens (`-`) are converted to underscores (`_`).
/// - Any other non-alphanumeric character is invalid and will cause an error.
fn registry_env_var_name(registry: &str) -> anyhow::Result<String> {
    let mut sanitized_name = String::with_capacity(registry.len());

    for ch in registry.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            sanitized_name.push(ch);
        } else if ch == '-' {
            sanitized_name.push('_');
        } else {
            anyhow::bail!("Invalid character in registry name `{registry}`: `{ch}`");
        }
    }

    Ok(sanitized_name.to_uppercase())
}

/// Find the URL of a registry
pub fn registry_url(manifest_path: &Path, registry: Option<&str>) -> anyhow::Result<Url> {
    fn read_config(
        registries: &mut HashMap<String, Source>,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        // TODO unit test for source replacement
        let content = fs_err::read_to_string(path).context("failed to read cargo config file")?;
        let config = toml::from_str::<CargoConfig>(&content).context("Invalid cargo config")?;
        for (key, value) in config.registries {
            registries.entry(key).or_insert(Source {
                registry: value.index,
                replace_with: None,
            });
        }
        for (key, value) in config.source {
            registries.entry(key).or_insert(value);
        }
        Ok(())
    }
    // registry might be replaced with another source
    // it's looks like a singly linked list
    // put relations in this map.
    let mut registries: HashMap<String, Source> = HashMap::new();

    // set top-level env var override if it exists.
    if let Some(registry_name) = registry
        && let Some(env_var_override) = registry_index_url_from_env(registry_name)?
    {
        registries
            .entry(registry_name.to_string())
            .or_insert(Source {
                registry: Some(env_var_override),
                replace_with: None,
            });
    }

    // ref: https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure
    for work_dir in manifest_path
        .parent()
        .expect("there must be a parent directory")
        .ancestors()
    {
        let work_cargo_dir = work_dir.join(".cargo");
        let config_path = work_cargo_dir.join("config");
        if config_path.is_file() {
            read_config(&mut registries, config_path)?;
        } else {
            let config_path = work_cargo_dir.join("config.toml");
            if config_path.is_file() {
                read_config(&mut registries, config_path)?;
            }
        }
    }

    let default_cargo_home = cargo_home()?;
    let default_config_path = default_cargo_home.join("config");
    if default_config_path.is_file() {
        read_config(&mut registries, default_config_path)?;
    } else {
        let default_config_path = default_cargo_home.join("config.toml");
        if default_config_path.is_file() {
            read_config(&mut registries, default_config_path)?;
        }
    }

    // find head of the relevant linked list
    let mut source = match registry {
        Some(CRATES_IO_INDEX) | None => {
            let mut source = registries.remove(CRATES_IO_REGISTRY).unwrap_or_default();
            source
                .registry
                .get_or_insert_with(|| CRATES_IO_INDEX.to_string());
            source
        }
        Some(r) => registries
            .remove(r)
            .with_context(|| anyhow::anyhow!("The registry '{r}' could not be found"))?,
    };

    // search this linked list and find the tail
    while let Some(replace_with) = &source.replace_with {
        let is_crates_io = replace_with == CRATES_IO_INDEX;
        source = registries
            .remove(replace_with)
            .with_context(|| anyhow::anyhow!("The source '{replace_with}' could not be found"))?;
        if is_crates_io {
            source
                .registry
                .get_or_insert_with(|| CRATES_IO_INDEX.to_string());
        }
    }

    let registry_url = source
        .registry
        .and_then(|x| Url::parse(&x).ok())
        .context("Invalid cargo config")?;

    Ok(registry_url)
}

#[derive(Debug, Deserialize)]
struct CargoConfig {
    #[serde(default)]
    registries: HashMap<String, Registry>,
    #[serde(default)]
    source: HashMap<String, Source>,
}

#[derive(Default, Debug, Deserialize)]
struct Source {
    #[serde(rename = "replace-with")]
    replace_with: Option<String>,
    registry: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Registry {
    index: Option<String>,
}

pub fn cargo_home() -> anyhow::Result<PathBuf> {
    let default_cargo_home = dirs::home_dir()
        .map(|x| x.join(".cargo"))
        .context("Failed to read home directory")?;
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or(default_cargo_home);
    Ok(cargo_home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_env_var_name() {
        assert_eq!(
            cargo_registries_index_env_var_name("my-registry").unwrap(),
            "CARGO_REGISTRIES_MY_REGISTRY_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("my_registry").unwrap(),
            "CARGO_REGISTRIES_MY_REGISTRY_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("registry1").unwrap(),
            "CARGO_REGISTRIES_REGISTRY1_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("UPPERCASE").unwrap(),
            "CARGO_REGISTRIES_UPPERCASE_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("-leading-dash").unwrap(),
            "CARGO_REGISTRIES__LEADING_DASH_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("trailing-dash-").unwrap(),
            "CARGO_REGISTRIES_TRAILING_DASH__INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("multiple---dashes").unwrap(),
            "CARGO_REGISTRIES_MULTIPLE___DASHES_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("mixed-dashes-and_underscores").unwrap(),
            "CARGO_REGISTRIES_MIXED_DASHES_AND_UNDERSCORES_INDEX"
        );
        assert_eq!(
            cargo_registries_index_env_var_name("---").unwrap(),
            "CARGO_REGISTRIES_____INDEX"
        );

        // Invalid characters should fail

        expect_test::expect!["Invalid character in registry name `with-special-chars!`: `!`"]
            .assert_eq(&registry_env_var_name_error("with-special-chars!"));

        expect_test::expect!["Invalid character in registry name `invalid+char`: `+`"]
            .assert_eq(&registry_env_var_name_error("invalid+char"));

        expect_test::expect!["Invalid character in registry name `has@symbol`: `@`"]
            .assert_eq(&registry_env_var_name_error("has@symbol"));

        expect_test::expect!["Invalid character in registry name `space not allowed`: ` `"]
            .assert_eq(&registry_env_var_name_error("space not allowed"));
    }

    fn registry_env_var_name_error(registry: &str) -> String {
        cargo_registries_index_env_var_name(registry)
            .unwrap_err()
            .to_string()
    }
}
