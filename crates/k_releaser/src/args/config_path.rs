use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use clap::Args;
use fs_err::read_to_string;
use tracing::info;

use crate::config::Config;

/// A clap [`Args`] struct that specifies the path to the Cargo.toml file containing k-releaser config.
#[derive(Debug, Default, Args)]
pub struct ConfigPath {
    /// Path to the Cargo.toml file containing k-releaser configuration in [package.metadata.k-releaser].
    ///
    /// If not specified, looks for ./Cargo.toml in the current directory.
    ///
    /// If no config is found in Cargo.toml, the default configuration is used.
    #[arg(long = "config", value_name = "PATH")]
    path: Option<PathBuf>,
}

impl ConfigPath {
    /// Load the k-releaser configuration from Cargo.toml [package.metadata.k-releaser] section.
    ///
    /// If a path is specified, it will attempt to load the configuration from that Cargo.toml file.
    /// If the file does not exist, it will return an error. If no path is specified, it will check
    /// for ./Cargo.toml in the current directory.
    pub fn load(&self) -> anyhow::Result<Config> {
        let cargo_toml_path = if let Some(path) = self.path.as_deref() {
            path.to_path_buf()
        } else {
            Path::new("Cargo.toml").to_path_buf()
        };

        match load_config_from_cargo_toml(&cargo_toml_path) {
            Ok(Some(config)) => Ok(config),
            Ok(None) => {
                // If path was explicitly specified but the file doesn't exist, return error
                if self.path.is_some() && !cargo_toml_path.exists() {
                    return Err(anyhow::anyhow!(
                        "failed to read config from {}: file not found",
                        cargo_toml_path.display()
                    ));
                }
                info!(
                    "No k-releaser configuration found in {}, using default configuration",
                    cargo_toml_path.display()
                );
                Ok(Config::default())
            }
            Err(err) if self.path.is_some() => {
                Err(err.context(format!(
                    "failed to read config from {}",
                    cargo_toml_path.display()
                )))
            }
            Err(_) => {
                info!(
                    "Cargo.toml not found at {}, using default configuration",
                    cargo_toml_path.display()
                );
                Ok(Config::default())
            }
        }
    }
}

/// Try to load the configuration from Cargo.toml's [package.metadata.k-releaser] or
/// [workspace.metadata.k-releaser] section.
///
/// Returns `Ok(Some(config))` if the metadata is found and valid, `Ok(None)` if no metadata exists,
/// and an error if the file exists but is invalid.
fn load_config_from_cargo_toml(path: &Path) -> anyhow::Result<Option<Config>> {
    match read_to_string(path) {
        Ok(contents) => {
            let cargo_toml: toml::Value = toml::from_str(&contents)
                .with_context(|| format!("invalid Cargo.toml at {}", path.display()))?;

            // Try to extract [workspace.metadata.k-releaser] first, then [package.metadata.k-releaser]
            let metadata = cargo_toml
                .get("workspace")
                .and_then(|w| w.get("metadata"))
                .and_then(|m| m.get("k-releaser"))
                .or_else(|| {
                    cargo_toml
                        .get("package")
                        .and_then(|p| p.get("metadata"))
                        .and_then(|m| m.get("k-releaser"))
                });

            if let Some(metadata) = metadata {
                let config = metadata
                    .clone()
                    .try_into()
                    .with_context(|| {
                        format!(
                            "invalid k-releaser configuration in metadata at {}",
                            path.display()
                        )
                    })?;
                info!(
                    "using k-releaser config from Cargo.toml metadata in {}",
                    path.display()
                );
                Ok(Some(config))
            } else {
                Ok(None)
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::{NamedTempFile, tempdir};

    use super::*;

    #[test]
    fn load_config_with_specified_path_success() {
        let temp_file = NamedTempFile::new().unwrap();
        let default_config = toml::to_string(&Config::default()).unwrap();
        let cargo_toml = format!(
            r#"
[package]
name = "test"
version = "0.1.0"

[package.metadata.k-releaser]
{}
"#,
            default_config
        );
        fs_err::write(&temp_file, cargo_toml).unwrap();

        let config_path = ConfigPath {
            path: Some(temp_file.path().to_path_buf()),
        };

        assert_eq!(config_path.load().unwrap(), Config::default());
    }

    #[test]
    fn load_config_with_specified_path_not_found() {
        let temp_dir = tempdir().unwrap();
        let non_existent_path = temp_dir.path().join("Cargo.toml");

        let config_path = ConfigPath {
            path: Some(non_existent_path),
        };

        let result = config_path.load().unwrap_err();
        assert!(result.to_string().contains("failed to read config from"));
    }

    #[test]
    fn load_config_with_invalid_toml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid toml content [[[").unwrap();

        let config_path = ConfigPath {
            path: Some(temp_file.path().to_path_buf()),
        };

        let result = format!("{:?}", config_path.load().unwrap_err());
        assert!(result.contains("invalid Cargo.toml"));
    }

    #[test]
    fn load_config_default_path_success() {
        let temp_dir = tempdir().unwrap();
        let cargo_toml_path = temp_dir.path().join("Cargo.toml");
        let default_config = toml::to_string(&Config::default()).unwrap();
        let cargo_toml = format!(
            r#"
[package]
name = "test"
version = "0.1.0"

[package.metadata.k-releaser]
{}
"#,
            default_config
        );
        fs_err::write(&cargo_toml_path, cargo_toml).unwrap();

        // Change directory to temp_dir so Cargo.toml is found
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let config_path = ConfigPath { path: None };
        let result = config_path.load().unwrap();

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        assert_eq!(result, Config::default());
    }

    #[test]
    fn load_config_no_config_file_uses_default() {
        let temp_dir = tempdir().unwrap();

        // Change directory to temp_dir where no Cargo.toml exists
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let config_path = ConfigPath { path: None };

        // Ensure no Cargo.toml exists
        assert!(!temp_dir.path().join("Cargo.toml").exists());

        // Load the config, which should return the default
        let result = config_path.load().unwrap();

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        assert_eq!(result, Config::default());
    }
}
