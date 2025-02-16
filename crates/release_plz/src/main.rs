mod changelog_config;
mod commands;
mod config;
mod log;

use clap::{
    builder::{styling::AnsiColor, Styles},
    Parser,
};
use commands::Command;
use tracing::error;

const MAIN_COLOR: AnsiColor = AnsiColor::Red;
const SECONDARY_COLOR: AnsiColor = AnsiColor::Yellow;
const HELP_STYLES: Styles = Styles::styled()
    .header(MAIN_COLOR.on_default().bold())
    .usage(MAIN_COLOR.on_default().bold())
    .placeholder(SECONDARY_COLOR.on_default())
    .literal(SECONDARY_COLOR.on_default());

#[derive(clap::Parser, Debug)]
#[command( version, author, styles = HELP_STYLES)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Command,

    /// Print source location and additional information in logs.
    /// To change the log level, use the `RUST_LOG` environment variable.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    log::init(args.verbose);
    args.command.run().await.inspect_err(|e| error!("{:?}", e))
}
