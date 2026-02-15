use anyhow::Context;
use cargo_metadata::{Package, camino::Utf8Path};
use secrecy::{ExposeSecret, SecretString};
use std::{
    env,
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};
use tracing::{debug, info};
use url::Url;

pub struct CargoRegistry {
    /// Name of the registry.
    /// [`Option::None`] means default 'crate.io'.
    pub name: Option<String>,
    pub index_url: Option<Url>,
}

fn cargo_cmd() -> Command {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    Command::new(cargo)
}

pub fn run_cargo(root: &Utf8Path, args: &[&str]) -> anyhow::Result<CmdOutput> {
    run_cargo_with_env(root, args, &[])
}

pub fn run_cargo_with_env(
    root: &Utf8Path,
    args: &[&str],
    envs: &[(String, String)],
) -> anyhow::Result<CmdOutput> {
    debug!("Run `cargo {}` in {root}", args.join(" "));

    let mut command = cargo_cmd();
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }

    let output = command.output().context("cannot run cargo")?;

    let output_stdout = String::from_utf8(output.stdout)?;
    let output_stderr = String::from_utf8(output.stderr)?;

    debug!("cargo stderr: {}", output_stderr);
    debug!("cargo stdout: {}", output_stdout);

    Ok(CmdOutput {
        status: output.status,
        stdout: output_stdout,
        stderr: output_stderr,
    })
}

pub struct CmdOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Check if the package is published via `cargo info`.
pub async fn is_published(
    workspace_root: &Utf8Path,
    package: &Package,
    timeout: Duration,
    registry: Option<&str>,
    index_url: Option<&Url>,
    token: &Option<SecretString>,
) -> anyhow::Result<bool> {
    tokio::time::timeout(timeout, async {
        let output = run_cargo_info(workspace_root, package, registry, index_url, token)
            .context("cannot run cargo info")?;
        if output.status.success() {
            Ok(true)
        } else if cargo_info_reports_missing(&output) {
            Ok(false)
        } else {
            let error_output = if output.stderr.trim().is_empty() {
                output.stdout.trim()
            } else {
                output.stderr.trim()
            };
            anyhow::bail!(
                "cargo info failed for {}@{}: {}",
                package.name,
                package.version,
                error_output
            )
        }
    })
    .await?
    .with_context(|| format!("timeout while checking published {}", package.name))
}

fn cargo_info_registry_name(registry: Option<&str>) -> &str {
    match registry {
        None | Some("crates-io") => "crates-io",
        Some(name) => name,
    }
}

fn cargo_info_reports_missing(output: &CmdOutput) -> bool {
    // Cargo output for `cargo info` is not a stable API. We only match the
    // string we have observed in practice to avoid false positives.
    let haystack = format!("{}\n{}", output.stdout, output.stderr).to_lowercase();
    haystack.contains("could not find")
}

fn run_cargo_info(
    workspace_root: &Utf8Path,
    package: &Package,
    registry: Option<&str>,
    index_url: Option<&Url>,
    token: &Option<SecretString>,
) -> anyhow::Result<CmdOutput> {
    let registry_name = cargo_info_registry_name(registry);
    let mut args = vec![
        "info".to_string(),
        format!("{}@{}", package.name, package.version),
    ];

    if let Some(index_url) = index_url {
        args.push("--index".to_string());
        args.push(index_url.as_str().to_string());
    } else {
        args.push("--registry".to_string());
        args.push(registry_name.to_string());
    }

    debug!("Run `cargo {}` in {workspace_root}", args.join(" "));

    let mut cmd = cargo_cmd();
    cmd.current_dir(workspace_root).args(&args);

    if let Some(token) = token {
        if registry_name == "crates-io" {
            cmd.env("CARGO_REGISTRY_TOKEN", token.expose_secret());
        } else {
            let env_var = format!("CARGO_REGISTRIES_{}_TOKEN", registry_name.to_uppercase());
            cmd.env(env_var, token.expose_secret());
        }
    }

    let output = cmd.output().context("cannot run cargo info")?;
    let output_stdout = String::from_utf8(output.stdout)?;
    let output_stderr = String::from_utf8(output.stderr)?;

    debug!("cargo stderr: {}", output_stderr);
    debug!("cargo stdout: {}", output_stdout);

    Ok(CmdOutput {
        status: output.status,
        stdout: output_stdout,
        stderr: output_stderr,
    })
}

pub async fn wait_until_published(
    workspace_root: &Utf8Path,
    package: &Package,
    timeout: Duration,
    registry: Option<&str>,
    index_url: Option<&Url>,
    token: &Option<SecretString>,
) -> anyhow::Result<()> {
    let now: Instant = Instant::now();
    let sleep_time = Duration::from_secs(2);
    let mut logged = false;

    loop {
        let is_published =
            is_published(workspace_root, package, timeout, registry, index_url, token).await?;
        if is_published {
            break;
        } else if timeout < now.elapsed() {
            anyhow::bail!(
                "timeout of {:?} elapsed while publishing the package {}. You can increase this timeout by editing the `publish_timeout` field in the `release-plz.toml` file",
                timeout,
                package.name
            )
        }

        if !logged {
            info!(
                "waiting for the package {} to be published...",
                package.name
            );
            logged = true;
        }

        tokio::time::sleep(sleep_time).await;
    }

    Ok(())
}
