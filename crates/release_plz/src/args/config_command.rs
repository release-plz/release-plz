use anyhow::Context as _;
use cargo_metadata::camino::Utf8Path;

use crate::config::Config;

pub trait ConfigCommand {
    fn config_path(&self) -> Option<&Utf8Path>;

    fn config(&self) -> anyhow::Result<Config> {
        super::parse_config(self.config_path()).context("failed to parse release-plz configuration")
    }
}
