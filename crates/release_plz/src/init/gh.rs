use std::{process::Command, vec};

use anyhow::Context;

fn gh() -> Command {
    Command::new("gh")
}

fn gh_repo_view(query: &[&str]) -> anyhow::Result<String> {
    let mut args = vec!["repo", "view", "--json"];
    args.extend(query);

    let output = gh().args(args).output().context("error while running gh")?;
    let stdout = get_stdout_if_success(output)?;
    Ok(stdout)
}

pub fn repo_url() -> anyhow::Result<String> {
    gh_repo_view(&["url", "-q", ".url"]).context("error while retrieving repository url")
}

pub fn repo_owner() -> anyhow::Result<String> {
    gh_repo_view(&["owner", "-q", ".owner.login"])
        .context("error while retrieving repository owner")
}

/// Store secret reading it from stdin.
pub fn store_secret(token_name: &str) -> anyhow::Result<()> {
    let output = gh()
        .arg("secret")
        .arg("set")
        .arg(token_name)
        .spawn()
        .context("error while spawning gh to set repository secret")?
        .wait_with_output()
        .context("error while waiting gh to set repository secret")?;
    get_stdout_if_success(output).context("error while setting repository secret")?;
    println!();
    Ok(())
}

pub fn is_gh_installed() -> bool {
    gh().arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn default_branch() -> anyhow::Result<String> {
    gh_repo_view(&["defaultBranchRef", "--jq", ".defaultBranchRef.name"])
        .context("error while retrieving default branch")
}

fn get_stdout_if_success(output: std::process::Output) -> anyhow::Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).unwrap_or_default();
        anyhow::bail!("gh failed: {stderr}");
    }
    let stdout = String::from_utf8(output.stdout)
        .context("error while reading gh stdout")?
        .trim()
        .to_string();
    Ok(stdout)
}
