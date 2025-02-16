mod check_updates;
mod completions;
mod config;
mod init_github;
mod manifest;
mod pull_request;
mod release;
mod repo;
mod schema;
mod set_version;
mod update;

use clap::ValueEnum;
use serde::Serialize;

use self::{
    completions::Completions, init_github::InitGithub, pull_request::PullRequest, release::Release,
    set_version::SetVersion, update::Update,
};

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    #[command(alias = "init", long_about)]
    InitGithub(InitGithub),
    Update(Update),
    #[command(name = "pr", aliases = ["pull-request", "release-pr"])]
    PullRequest(PullRequest),
    SetVersion(SetVersion),
    Release(Release),
    #[command(alias = "generate-completions")]
    Completions(Completions),

    /// Check if a newer version of release-plz is available.
    CheckUpdates,

    /// Write the JSON schema of the release-plz.toml configuration to .schema/latest.json
    #[command(alias = "schema")]
    GenerateSchema,
}

impl Command {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Command::Update(command) => command.run().await?,
            Command::PullRequest(command) => command.run().await?,
            Command::Release(command) => command.run().await?,
            Command::Completions(command) => command.run(),
            Command::CheckUpdates => check_updates::check_update().await?,
            Command::GenerateSchema => schema::generate_schema_to_disk()?,
            Command::InitGithub(command) => command.run()?,
            Command::SetVersion(command) => command.run()?,
        }
        Ok(())
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputType {
    Json,
}

fn print_output(output_type: OutputType, output: impl Serialize) {
    match output_type {
        OutputType::Json => match serde_json::to_string(&output) {
            Ok(json) => println!("{json}"),
            Err(e) => tracing::error!("can't serialize release pr to json: {e}"),
        },
    }
}
