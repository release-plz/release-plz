mod args;
mod changelog_config;
mod config;
mod generate_schema;
pub mod init;
mod log;
mod update_checker;

use args::{OutputType, config_command::ConfigCommand as _};
use clap::Parser;
use release_plz_core::ReleaseRequest;
use serde::Serialize;
use tracing::error;

use crate::args::{CliArgs, Command, manifest_command::ManifestCommand as _};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    log::init(args.verbosity()?);
    run(args).await.map_err(|e| {
        error!("{:?}", e);
        e
    })?;

    Ok(())
}

async fn run(args: CliArgs) -> anyhow::Result<()> {
    match args.command {
        Command::Update(cmd_args) => {
            let cargo_metadata = cmd_args.cargo_metadata()?;
            let config = cmd_args.config()?;
            let update_request = cmd_args.update_request(&config, cargo_metadata)?;
            let (packages_update, _temp_repo) = release_plz_core::update(&update_request).await?;
            println!("{}", packages_update.summary());
        }
        Command::ReleasePr(cmd_args) => {
            anyhow::ensure!(
                cmd_args.update.git_token.is_some(),
                "please provide the git token with the --git-token cli argument."
            );
            let cargo_metadata = cmd_args.update.cargo_metadata()?;
            let config = cmd_args.update.config()?;
            let request = cmd_args.release_pr_req(&config, cargo_metadata)?;
            let release_pr = release_plz_core::release_pr(&request).await?;
            if let Some(output_type) = cmd_args.output {
                let prs = match release_pr {
                    Some(pr) => vec![pr],
                    None => vec![],
                };
                let prs_json = serde_json::json!({
                    "prs": prs
                });
                print_output(output_type, prs_json);
            }
        }
        Command::Release(cmd_args) => {
            let cargo_metadata = cmd_args.cargo_metadata()?;
            let config = cmd_args.config()?;
            let cmd_args_output = cmd_args.output;
            let request: ReleaseRequest = cmd_args.release_request(&config, cargo_metadata)?;
            let output = release_plz_core::release(&request)
                .await?
                .unwrap_or_default();
            if let Some(output_type) = cmd_args_output {
                print_output(output_type, output);
            }
        }
        Command::GenerateCompletions(cmd_args) => cmd_args.print(),
        Command::CheckUpdates => update_checker::check_update().await?,
        Command::GenerateSchema => generate_schema::generate_schema_to_disk()?,
        Command::Init(cmd_args) => init::init(&cmd_args.manifest_path(), !cmd_args.no_toml_check)?,
        Command::SetVersion(cmd_args) => {
            let config = cmd_args.config()?;
            let request = cmd_args.set_version_request(&config)?;
            release_plz_core::set_version::set_version(&request)?;
        }
    }
    Ok(())
}

fn print_output(output_type: OutputType, output: impl Serialize) {
    match output_type {
        OutputType::Json => match serde_json::to_string(&output) {
            Ok(json) => println!("{json}"),
            Err(e) => tracing::error!("can't serialize release pr to json: {e}"),
        },
    }
}
