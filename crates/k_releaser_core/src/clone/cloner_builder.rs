// Copied from [cargo-clone](https://github.com/JanLikar/cargo-clone/blob/89ba4da215663ffb3b8c93a674f3002937eafec4/cargo-clone-core/src/cloner_builder.rs)

use anyhow::Context;
use cargo::{CargoResult, GlobalContext, core::Shell, util::homedir};
use cargo_metadata::camino::Utf8PathBuf;

use crate::fs_utils::current_directory;

use super::{Cloner, ClonerSource};

/// Builder for [`Cloner`].
#[derive(Debug, Default)]
pub struct ClonerBuilder {
    config: Option<GlobalContext>,
    directory: Option<Utf8PathBuf>,
    source: ClonerSource,
    /// Cargo current working directory. You can use it to point to the right `.cargo/config.toml`.
    cargo_cwd: Option<Utf8PathBuf>,
    use_git: bool,
}

impl ClonerBuilder {
    /// Creates a new [`ClonerBuilder`] that:
    /// - Uses crates.io as source.
    /// - Clones the crates into the current directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clone into a different directory, instead of the current one.
    pub fn with_directory(self, directory: impl Into<Utf8PathBuf>) -> Self {
        Self {
            directory: Some(directory.into()),
            ..self
        }
    }

    /// Clone from an alternative source, instead of crates.io.
    pub fn with_source(self, source: ClonerSource) -> Self {
        Self { source, ..self }
    }

    /// Set cargo working directory.
    pub fn with_cargo_cwd(self, path: Utf8PathBuf) -> Self {
        Self {
            cargo_cwd: Some(path),
            ..self
        }
    }

    /// Build the [`Cloner`].
    pub fn build(self) -> CargoResult<Cloner> {
        let config = match self.config {
            Some(config) => config,
            None => new_cargo_config(self.cargo_cwd).context("Unable to get cargo config.")?,
        };

        let directory = match self.directory {
            Some(directory) => directory,
            None => current_directory()?,
        };

        let srcid = self
            .source
            .cargo_source
            .to_source_id(&config)
            .context("can't determine the source id")?;

        Ok(Cloner {
            config,
            directory,
            srcid,
            use_git: self.use_git,
        })
    }
}

fn new_cargo_config(cwd: Option<Utf8PathBuf>) -> anyhow::Result<GlobalContext> {
    match cwd {
        Some(cwd) => {
            let shell = Shell::new();
            let homedir = homedir(cwd.as_std_path()).context(
                "Cargo couldn't find your home directory. \
                 This probably means that $HOME was not set.",
            )?;
            Ok(GlobalContext::new(shell, cwd.into_std_path_buf(), homedir))
        }
        None => GlobalContext::default(),
    }
}
