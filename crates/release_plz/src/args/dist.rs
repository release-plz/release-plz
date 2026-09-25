use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};
use release_plz_core::{
    GitClient, GitForge, GitHub, ReleaseRequest,
    dist::{DistJob, DistRequest},
};
use secrecy::SecretString;

use super::{
    config_path::ConfigPath, manifest_command::ManifestCommand, repo_command::RepoCommand,
};

#[derive(clap::Parser, Debug)]
pub struct Dist {
    #[command(subcommand)]
    command: DistCommand,
}

#[derive(clap::Subcommand, Debug)]
enum DistCommand {
    /// Build binaries for this runner and upload them to the draft GitHub release.
    Build(DistArgs),
    /// Generate installers and download instructions, then publish after the matrix succeeds.
    Finalize(DistArgs),
}

#[derive(clap::Args, Debug)]
struct DistArgs {
    /// Release tag. Defaults to the tag in the `repository_dispatch` or `workflow_dispatch` event.
    #[arg(long)]
    tag: Option<String>,
    /// Path to Cargo.toml.
    #[arg(long)]
    manifest_path: Option<PathBuf>,
    #[command(flatten)]
    config: ConfigPath,
    /// GitHub repository URL. Defaults to the origin remote.
    #[arg(long)]
    repo_url: Option<String>,
    /// GitHub token with contents:write permission.
    #[arg(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    git_token: String,
}

impl Dist {
    pub async fn run(self) -> anyhow::Result<()> {
        let (args, finalize) = match self.command {
            DistCommand::Build(args) => (args, false),
            DistCommand::Finalize(args) => (args, true),
        };
        let config = args.config.load()?;
        let metadata = args.cargo_metadata()?;
        let repo_url = args.get_repo_url(&config)?;
        let repository = repo_url.full_host();
        let client = GitClient::new(GitForge::Github(GitHub::from_repo_url(
            repo_url,
            SecretString::from(args.git_token.clone()),
        )?))?;
        let release_config =
            config.fill_release_config(false, false, ReleaseRequest::new(metadata.clone()))?;
        let tag = match args.tag {
            Some(tag) => tag,
            None => event_tag()?,
        };
        ensure!(!tag.is_empty(), "release tag must not be empty");
        let request = DistRequest::new(metadata, &release_config, tag, client, repository)?;
        if finalize {
            request.finalize(&release_plz_core::dist::run_id()?).await
        } else {
            request.build(DistJob::from_env()?).await
        }
    }
}

fn event_tag() -> anyhow::Result<String> {
    let path = std::env::var_os("GITHUB_EVENT_PATH")
        .context("pass --tag or run from a distribution workflow event")?;
    let event: serde_json::Value = serde_json::from_slice(&fs_err::read(path)?)?;
    event
        .pointer("/client_payload/tag")
        .or_else(|| event.pointer("/inputs/tag"))
        .and_then(|tag| tag.as_str())
        .map(str::to_owned)
        .context("GitHub event has no distribution tag; pass --tag")
}

impl ManifestCommand for DistArgs {
    fn optional_manifest(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }
}

impl RepoCommand for DistArgs {
    fn repo_url(&self) -> Option<&str> {
        self.repo_url.as_deref()
    }
}
