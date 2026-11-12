use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::builder::{NonEmptyStringValueParser, PathBufValueParser};
use release_plz_core::{
    GitHub, ReleaseRequest,
    dist::{DistJob, DistRequest, run_id},
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
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    tag: Option<String>,
    /// Path to Cargo.toml.
    #[arg(long, value_parser = PathBufValueParser::new(), alias = "project-manifest")]
    manifest_path: Option<PathBuf>,
    #[command(flatten)]
    config: ConfigPath,
    /// GitHub repository URL. Defaults to the origin remote.
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    repo_url: Option<String>,
    /// GitHub token with contents:write permission.
    #[arg(long, value_parser = NonEmptyStringValueParser::new(), env = "GITHUB_TOKEN", hide_env_values = true)]
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
        let github = GitHub::from_repo_url(
            args.get_repo_url(&config)?,
            SecretString::from(args.git_token),
        )?;
        let request = config.fill_release_config(false, false, ReleaseRequest::new(metadata))?;
        let tag = match args.tag {
            Some(tag) => tag,
            None => event_tag()?,
        };
        let request = DistRequest::new(request, tag, github)?;
        if finalize {
            request.finalize(&run_id()?).await
        } else {
            request.build(DistJob::from_env()?).await
        }
    }
}

fn event_tag() -> anyhow::Result<String> {
    let path = std::env::var_os("GITHUB_EVENT_PATH")
        .context("pass --tag or run from a distribution workflow event")?;
    let event: serde_json::Value = serde_json::from_slice(&fs_err::read(path)?)?;
    tag_from_event(&event)
}

/// The tag of a `repository_dispatch` (`client_payload`) or `workflow_dispatch` (`inputs`) event.
fn tag_from_event(event: &serde_json::Value) -> anyhow::Result<String> {
    event
        .pointer("/client_payload/tag")
        .or_else(|| event.pointer("/inputs/tag"))
        .and_then(|tag| tag.as_str())
        .filter(|tag| !tag.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_tag_comes_from_dispatch_payload_or_workflow_inputs() {
        let dispatch = json!({"action": "release-plz-dist", "client_payload": {"tag": "v1.0.0"}});
        assert_eq!(tag_from_event(&dispatch).unwrap(), "v1.0.0");
        let workflow = json!({"inputs": {"tag": "app-v2.0.0"}});
        assert_eq!(tag_from_event(&workflow).unwrap(), "app-v2.0.0");
        for event in [
            json!({"action": "published", "release": {"tag_name": "v1.0.0"}}),
            json!({"client_payload": {"tag": 1}}),
            json!({"inputs": {"tag": null}}),
            json!({"inputs": {"tag": ""}}),
        ] {
            let error = tag_from_event(&event).unwrap_err().to_string();
            assert!(
                error.contains("GitHub event has no distribution tag; pass --tag"),
                "{error}"
            );
        }
    }
}
