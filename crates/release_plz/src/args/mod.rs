pub(crate) mod config_command;
mod generate_completions;
mod init;
pub(crate) mod manifest_command;
mod release;
mod release_pr;
pub(crate) mod repo_command;
mod set_version;
mod update;

use std::path::Path;

use anyhow::{Context, bail};
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use clap::{
    ValueEnum,
    builder::{Styles, styling::AnsiColor},
};
use init::Init;
use release_plz_core::fs_utils::current_directory;
use set_version::SetVersion;
use tracing::{info, level_filters::LevelFilter};

use crate::config::Config;

use self::{
    generate_completions::GenerateCompletions, release::Release, release_pr::ReleasePr,
    update::Update,
};

const MAIN_COLOR: AnsiColor = AnsiColor::Red;
const SECONDARY_COLOR: AnsiColor = AnsiColor::Yellow;
const HELP_STYLES: Styles = Styles::styled()
    .header(MAIN_COLOR.on_default().bold())
    .usage(MAIN_COLOR.on_default().bold())
    .placeholder(SECONDARY_COLOR.on_default())
    .literal(SECONDARY_COLOR.on_default());

/// Release-plz manages versioning, changelogs, and releases for Rust projects.
///
/// See the Release-plz website for more information <https://release-plz.dev/>.
#[derive(clap::Parser, Debug)]
#[command(version, author, styles = HELP_STYLES)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Command,
    /// Print source location and additional information in logs.
    ///
    /// If this option is unspecified, logs are printed at the INFO level without verbosity.
    /// `-v` adds verbosity to logs.
    /// `-vv` adds verbosity and sets the log level to DEBUG.
    /// `-vvv` adds verbosity and sets the log level to TRACE.
    /// To change the log level without setting verbosity, use the `RELEASE_PLZ_LOG`
    /// environment variable. E.g. `RELEASE_PLZ_LOG=DEBUG`.
    #[arg(
        short,
        long,
        global = true,
        action = clap::ArgAction::Count,
    )]
    verbose: u8,
}

impl CliArgs {
    pub fn verbosity(&self) -> anyhow::Result<Option<LevelFilter>> {
        let level = match self.verbose {
            0 => None,
            1 => Some(LevelFilter::INFO),
            2 => Some(LevelFilter::DEBUG),
            3 => Some(LevelFilter::TRACE),
            _ => bail!("invalid verbosity level. Use -v, -vv, or -vvv."),
        };
        Ok(level)
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Update packages version and changelogs based on commit messages.
    Update(Update),
    /// Create a Pull Request representing the next release.
    /// The Pull request contains updated packages version and changelog based on commit messages.
    /// Close old PRs opened by release-plz, too.
    ReleasePr(ReleasePr),
    /// For each package not published to the cargo registry yet:
    /// - create and push upstream a tag in the format of `<package>-v<version>`.
    /// - publish the package to the cargo registry.
    ///
    /// You can run this command in the CI on every commit in the main branch.
    Release(Release),
    /// Generate command autocompletions for various shells.
    GenerateCompletions(GenerateCompletions),
    /// Check if a newer version of release-plz is available.
    CheckUpdates,
    /// Write the JSON schema of the release-plz.toml configuration
    /// to .schema/latest.json
    GenerateSchema,
    /// Initialize release-plz for the current GitHub repository, by storing the necessary tokens in the GitHub repository secrets and generating the release-plz.yml GitHub Actions workflow file.
    Init(Init),
    /// Edit the version of a package in Cargo.toml and changelog.
    /// Specify a version with the syntax `<package_name>@<version>`.
    /// E.g. `release-plz set-version rand@1.2.3`
    ///
    /// You can also set multiple versions, separated by space.
    /// E.g. `release-plz set-version rand@1.2.3 serde@2.0.0`
    ///
    /// For single package projects, you can omit `<package_name>@`. E.g. `release-plz set-version 1.2.3`
    ///
    /// Note that this command is meant to edit the versions of the packages
    /// of your workspace, not the version of your dependencies.
    SetVersion(SetVersion),
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputType {
    Json,
}

fn local_manifest(manifest_path: Option<&Utf8Path>) -> Utf8PathBuf {
    match manifest_path {
        Some(manifest) => manifest.to_path_buf(),
        None => current_directory().unwrap().join(CARGO_TOML),
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
