use std::path::Path;

use anyhow::Context as _;
use tracing::info;

use crate::config::Config;

pub trait ConfigCommand {
    fn config_path(&self) -> Option<&Path>;

    fn config(&self) -> anyhow::Result<Config> {
        parse_config(self.config_path()).context("failed to parse release-plz configuration")
    }
}

fn parse_config(config_path: Option<&Path>) -> anyhow::Result<Config> {
    let (config, path) = if let Some(config_path) = config_path {
        match fs_err::read_to_string(config_path) {
            Ok(config) => (config, config_path),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    anyhow::bail!("specified config does not exist at path {config_path:?}")
                }
                _ => anyhow::bail!("can't read {config_path:?}: {e:?}"),
            },
        }
    } else {
        let first_file = first_file_contents([
            Path::new("release-plz.toml"),
            Path::new(".release-plz.toml"),
        ])
        .context("failed looking for release-plz config file")?;
        match first_file {
            Some((config, path)) => (config, path),
            None => {
                info!("release-plz config file not found, using default configuration");
                return Ok(Config::default());
            }
        }
    };

    info!("using release-plz config file {}", path.display());
    toml::from_str(&config).with_context(|| format!("invalid config file {config_path:?}"))
}

/// Returns the contents of the first file that exists.
///
/// If none of the files exist, returns `Ok(None)`.
///
/// # Errors
///
/// Errors if opening and reading one of files paths fails for reasons other that it doesn't exist.
fn first_file_contents<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> anyhow::Result<Option<(String, &'a Path)>> {
    let paths = paths.into_iter();

    for path in paths {
        match fs_err::read_to_string(path) {
            Ok(config) => return Ok(Some((config, path))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }

    Ok(None)
}
