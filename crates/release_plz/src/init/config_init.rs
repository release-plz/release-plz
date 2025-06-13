use anyhow::Context;

use crate::config::Config;

/// Creates a default release-plz.toml file with basic configuration
pub fn create_default_config() -> anyhow::Result<()> {
    for config_path in crate::args::config_paths() {
        if config_path.exists() {
            println!("Config file already exists at {config_path:?}");
            return Ok(());
        }
    }

    write_config()?;

    Ok(())
}

fn write_config() -> anyhow::Result<()> {
    let default_config = Config::default();
    let toml_string =
        toml::to_string(&default_config).context("Failed to serialize config to TOML")?;
    let config_path = crate::args::main_config_path();
    fs_err::write(config_path, toml_string).context("Failed to write config to file")?;
    println!("Created default config file at {config_path}");
    Ok(())
}
