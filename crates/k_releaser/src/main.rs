mod args;
mod changelog_config;
mod config;
pub mod init;
mod log;

use args::OutputType;
use clap::Parser;
use k_releaser_core::ReleaseRequest;
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
            let config = cmd_args.config.load()?;
            let update_request = cmd_args.update_request(&config, cargo_metadata)?;
            let (packages_update, _temp_repo) = k_releaser_core::update(&update_request).await?;
            println!("{}", packages_update.summary());
        }
        Command::ReleasePr(cmd_args) => {
            anyhow::ensure!(
                cmd_args.update.git_token.is_some(),
                "please provide the git token with the --git-token cli argument."
            );
            let cargo_metadata = cmd_args.update.cargo_metadata()?;
            let config = cmd_args.update.config.load()?;
            let request = cmd_args.release_pr_req(&config, cargo_metadata)?;
            let release_pr = k_releaser_core::release_pr(&request).await?;
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
        Command::Publish(cmd_args) => {
            let cargo_metadata = cmd_args.cargo_metadata()?;
            let config = cmd_args.config.load()?;
            let print_order = cmd_args.print_order;
            let cmd_args_output = cmd_args.output;
            let request = cmd_args.publish_request(&config, cargo_metadata)?;

            if print_order {
                let order_output = k_releaser_core::print_publish_order(&request)?;
                if let Some(output_type) = cmd_args_output {
                    print_output(output_type, order_output);
                } else {
                    println!("{}", order_output.display());
                }
            } else {
                let output = k_releaser_core::publish(&request)
                    .await?
                    .unwrap_or_default();
                if let Some(output_type) = cmd_args_output {
                    print_output(output_type, output);
                }
            }
        }
        Command::Release(cmd_args) => {
            let cargo_metadata = cmd_args.cargo_metadata()?;
            let config = cmd_args.config.load()?;
            let cmd_args_output = cmd_args.output;
            let request: ReleaseRequest = cmd_args.release_request(&config, cargo_metadata)?;
            let output = k_releaser_core::release(&request)
                .await?
                .unwrap_or_default();
            if let Some(output_type) = cmd_args_output {
                print_output(output_type, output);
            }
        }
        Command::Init(cmd_args) => init::init(&cmd_args.manifest_path(), !cmd_args.no_toml_check)?,
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
