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
