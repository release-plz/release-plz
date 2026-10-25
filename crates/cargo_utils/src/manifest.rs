use anyhow::Context;

use std::str;

/// A Cargo manifest
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Manifest contents as TOML data
    pub data: toml_edit::DocumentMut,
}

impl str::FromStr for Manifest {
    type Err = anyhow::Error;

    /// Read manifest data from string
    fn from_str(input: &str) -> ::std::result::Result<Self, Self::Err> {
        let d: toml_edit::DocumentMut = input.parse().context("Manifest not valid TOML")?;

        Ok(Self { data: d })
    }
}

impl std::fmt::Display for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.data.to_string();
        s.fmt(f)
    }
}
