use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use clap::Args;
use fs_err::read_to_string;
use tracing::info;

use crate::config::Config;

const DEFAULT_CONFIG_PATHS: &[&str] = &["release-plz.toml", ".release-plz.toml"];

/// A clap [`Args`] struct that specifies the path to the release-plz config file.
#[derive(Debug, Default, Args)]
pub struct ConfigPath {
    /// Path to the release-plz config file.
    ///
    /// If not specified, the following paths are checked in order: `./release-plz.toml`,
    /// `./.release-plz.toml`
    ///
    /// If a config file is not found, the default configuration is used.
    #[arg(long = "config", value_name = "PATH")]
    path: Option<PathBuf>,
}

impl ConfigPath {
    /// Load the release-plz configuration from the specified path or default paths.
    ///
    /// If a path is specified, it will attempt to load the configuration from that file. If the
    /// file does not exist, it will return an error. If no path is specified, it will check the
    /// default paths (`release-plz.toml` and `.release-plz.toml`) and load the first one that
    /// exists.
    pub fn load(&self) -> anyhow::Result<Config> {
        if let Some(path) = self.path.as_deref() {
            match load_config(path) {
                Ok(Some(config)) => return Ok(config),
                Ok(None) => bail!("specified config file {} does not exist", path.display()),
                Err(err) => return Err(err.context("failed to read config file")),
            }
        }

        for path in DEFAULT_CONFIG_PATHS {
            let path = Path::new(path);
            if let Ok(Some(config)) = load_config(path) {
                return Ok(config);
            }
        }

        info!("release-plz config file not found, using default configuration");
        Ok(Config::default())
    }
}

/// Try to load the configuration from the specified path.
///
/// Returns `Ok(Some(config))` if the file is found and valid, `Ok(None)` if the file does not exist,
/// and an error if the file exists but is invalid.
fn load_config(path: &Path) -> anyhow::Result<Option<Config>> {
    match read_to_string(path) {
        Ok(contents) => {
            let config = toml::from_str(&contents)
                .with_context(|| format!("invalid config file {}", path.display()))?;
            info!("using release-plz config file {}", path.display());
            Ok(Some(config))
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
        fs_err::write(&temp_file, default_config).unwrap();

        let config_path = ConfigPath {
            path: Some(temp_file.path().to_path_buf()),
        };

        assert_eq!(config_path.load().unwrap(), Config::default());
    }

    #[test]
    fn load_config_with_specified_path_not_found() {
        let temp_dir = tempdir().unwrap();
        let non_existent_path = temp_dir.path().join("non-existent.toml");

        let config_path = ConfigPath {
            path: Some(non_existent_path),
        };

        let result = config_path.load().unwrap_err();
        assert!(result.to_string().contains("specified config file"));
    }

    #[test]
    fn load_config_with_invalid_toml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid toml content [[[").unwrap();

        let config_path = ConfigPath {
            path: Some(temp_file.path().to_path_buf()),
        };

        let result = format!("{:?}", config_path.load().unwrap_err());
        assert!(result.contains("invalid config file"));
    }

    #[test]
    fn load_config_default_path_success() {
        let temp_dir = tempdir().unwrap();
        let default_config_path = temp_dir.path().join("release-plz.toml");
        let default_config = toml::to_string(&Config::default()).unwrap();
        fs_err::write(&default_config_path, default_config).unwrap();

        let config_path = ConfigPath { path: None };

        assert_eq!(config_path.load().unwrap(), Config::default());
    }

    #[test]
    fn load_config_no_config_file_uses_default() {
        let temp_dir = tempdir().unwrap();
        let config_path = ConfigPath { path: None };

        // Ensure no config file exists
        assert!(!temp_dir.path().join("release-plz.toml").exists());
        assert!(!temp_dir.path().join(".release-plz.toml").exists());

        // Load the config, which should return the default
        assert_eq!(config_path.load().unwrap(), Config::default());
    }
}
