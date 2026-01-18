mod config_path;
mod init;
pub(crate) mod manifest_command;
mod publish;
mod release;
mod release_pr;
pub(crate) mod repo_command;
mod update;

use anyhow::bail;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_utils::CARGO_TOML;
use clap::{
    ValueEnum,
    builder::{Styles, styling::AnsiColor},
};
use init::Init;
use k_releaser_core::fs_utils::current_directory;
use tracing::level_filters::LevelFilter;

use self::{
    publish::Publish, release::Release, release_pr::ReleasePr, update::Update,
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
    /// To change the log level without setting verbosity, use the `K_RELEASER_LOG`
    /// environment variable. E.g. `K_RELEASER_LOG=DEBUG`.
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
    ///
    /// The Pull request updates the package version and generates a changelog entry for the new
    /// version based on the commit messages.
    /// If there is a previously opened Release PR, release-plz will update it
    /// instead of opening a new one.
    ReleasePr(ReleasePr),
    /// Publish packages to cargo registry.
    ///
    /// For each package not yet published to the cargo registry, publish the package.
    /// Packages are published in dependency order (dependencies first).
    ///
    /// This command only handles cargo registry publishing. Use the `release` command
    /// to create git tags and forge releases.
    Publish(Publish),
    /// Create git tags and forge releases.
    ///
    /// For each package, create and push upstream a tag in the format of `<package>-v<version>`,
    /// and create a release on the git forge (GitHub/GitLab/Gitea).
    ///
    /// This command does NOT publish to cargo registry. Use the `publish` command for that.
    ///
    /// You can run this command in the CI on every commit in the main branch.
    Release(Release),
    /// Initialize k-releaser for the current GitHub repository.
    ///
    /// Stores the necessary tokens in the GitHub repository secrets and generates the
    /// k-releaser.yml GitHub Actions workflow file.
    Init(Init),
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
