use anyhow::Context;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;

const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";
const CRATES_IO_REGISTRY: &str = "crates-io";

/// Read index for a specific registry using environment variables.
/// <https://doc.rust-lang.org/cargo/reference/environment-variables.html>
pub fn registry_index_url_from_env(registry: &str) -> Option<String> {
    let env_var = format!("CARGO_REGISTRIES_{}_INDEX", registry.to_uppercase());

    std::env::var(env_var).ok()
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
    if let Some(registry_name) = registry {
        if let Some(env_var_override) = registry_index_url_from_env(registry_name) {
            registries
                .entry(registry_name.to_string())
                .or_insert(Source {
                    registry: Some(env_var_override),
                    replace_with: None,
                });
        }
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
            .with_context(|| anyhow::anyhow!("The registry '{}' could not be found", r))?,
    };

    // search this linked list and find the tail
    while let Some(replace_with) = &source.replace_with {
        let is_crates_io = replace_with == CRATES_IO_INDEX;
        source = registries
            .remove(replace_with)
            .with_context(|| anyhow::anyhow!("The source '{}' could not be found", replace_with))?;
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
