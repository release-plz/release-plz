use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use clap::Args;
use fs_err::read_to_string;
use tracing::info;

use crate::config::Config;

/// Paths checked in order when `--config` is not specified.
const DEFAULT_CONFIG_PATHS: &[&str] = &[
    "release-plz.toml",
    ".release-plz.toml",
    ".config/release-plz.toml",
];

/// A clap [`Args`] struct that specifies the path to the release-plz config file.
#[derive(Debug, Default, Args)]
pub struct ConfigPath {
    /// Path to the release-plz config file.
    ///
    /// If not specified, the following paths are checked in order: `release-plz.toml`,
    /// `.release-plz.toml`, `.config/release-plz.toml`.
    ///
    /// If a config file is not found, the default configuration is used.
    #[arg(long = "config", value_name = "PATH")]
    path: Option<PathBuf>,
}

impl ConfigPath {
    /// Load the release-plz configuration from the specified path or default paths.
    ///
    /// If a path is specified, it will attempt to load the configuration from that file. If the
    /// file does not exist, it will return an error. If no path is specified, it will check
    /// [`DEFAULT_CONFIG_PATHS`] in order and load the first one that exists.
    pub fn load(&self) -> anyhow::Result<Config> {
        if let Some(path) = self.path.as_deref() {
            match load_config(path) {
                Ok(Some(config)) => return Ok(config),
                Ok(None) => bail!("specified config file {} does not exist", path.display()),
                Err(err) => return Err(err.context("failed to read config file")),
            }
        }

        load_default_config_in(Path::new("."))
    }
}

/// Load the first existing config file among [`DEFAULT_CONFIG_PATHS`], relative to `directory`.
///
/// Returns the default configuration if none of them exists.
fn load_default_config_in(directory: &Path) -> anyhow::Result<Config> {
    for path in DEFAULT_CONFIG_PATHS {
        let path = directory.join(path);
        match load_config(&path) {
            Ok(Some(config)) => return Ok(config),
            Ok(None) => (),
            Err(err) => return Err(err.context("invalid config file")),
        }
    }

    info!("release-plz config file not found, using default configuration");
    Ok(Config::default())
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
        // `NotADirectory` is returned when a parent component of the path (e.g. `.config`) is a
        // regular file. The config file doesn't exist in that case either.
        Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(None)
        }
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::{NamedTempFile, tempdir};

    use super::*;

    /// Write a config file at `relative_path` inside `dir`, creating parent directories.
    fn write_config(dir: &Path, relative_path: &str, contents: &str) {
        let path = dir.join(relative_path);
        fs_err::create_dir_all(path.parent().unwrap()).unwrap();
        fs_err::write(path, contents).unwrap();
    }

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
    fn load_config_no_config_file_uses_default() {
        let temp_dir = tempdir().unwrap();

        let config = load_default_config_in(temp_dir.path()).unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn load_config_uses_first_existing_default_path() {
        for (i, &expected) in DEFAULT_CONFIG_PATHS.iter().enumerate() {
            let temp_dir = tempdir().unwrap();
            // Create this path and every lower-priority one, marking each file with its own path.
            for path in &DEFAULT_CONFIG_PATHS[i..] {
                write_config(
                    temp_dir.path(),
                    path,
                    &format!("[workspace]\npr_branch_prefix = \"{path}\"\n"),
                );
            }

            let config = load_default_config_in(temp_dir.path()).unwrap();

            assert_eq!(config.workspace.pr_branch_prefix.as_deref(), Some(expected));
        }
    }

    #[test]
    fn load_config_ignores_dot_config_regular_file() {
        let temp_dir = tempdir().unwrap();
        fs_err::write(temp_dir.path().join(".config"), "").unwrap();

        let config = load_default_config_in(temp_dir.path()).unwrap();

        assert_eq!(config, Config::default());
    }
}
