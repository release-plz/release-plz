use anyhow::Context;
use cargo::{
    core::{SourceId, dependency::Dependency},
    sources::{
        SourceConfigMap,
        source::{QueryKind, Source},
    },
    util::{CargoResult, GlobalContext, cache_lock::CacheLockMode, homedir},
};
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
};
use tracing::{debug, info};

use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::HashSet,
    env,
    process::{Command, ExitStatus},
    sync::{LazyLock, Mutex},
    task::Poll,
    time::{Duration, Instant},
};

const CARGO_REGISTRY_TOKEN_ENV_VAR: &str = "CARGO_REGISTRY_TOKEN";

static TOKEN_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(Mutex::default);

pub struct CargoRegistry {
    /// Name of the registry.
    /// [`Option::None`] means default 'crate.io'.
    pub name: Option<String>,
    pub index: CargoIndex,
}

#[derive(Debug)]
pub enum CargoIndex {
    CratesIo {
        cargo_cwd: Utf8PathBuf,
    },
    Registry {
        name: String,
        cargo_cwd: Utf8PathBuf,
    },
}

impl CargoIndex {
    pub fn crates_io(cargo_cwd: Utf8PathBuf) -> Self {
        Self::CratesIo { cargo_cwd }
    }

    pub fn registry(name: String, cargo_cwd: Utf8PathBuf) -> Self {
        Self::Registry { name, cargo_cwd }
    }

    fn cargo_cwd(&self) -> &Utf8Path {
        match self {
            Self::CratesIo { cargo_cwd } | Self::Registry { cargo_cwd, .. } => cargo_cwd,
        }
    }

    fn source_id(&self, config: &GlobalContext) -> CargoResult<SourceId> {
        match self {
            Self::CratesIo { .. } => SourceId::crates_io(config),
            Self::Registry { name, .. } => SourceId::alt_registry(config, name),
        }
    }

    fn token_env_var_name(&self) -> anyhow::Result<String> {
        match self {
            Self::CratesIo { .. } => Ok(CARGO_REGISTRY_TOKEN_ENV_VAR.to_owned()),
            Self::Registry { name, .. } => cargo_utils::cargo_registries_token_env_var_name(name),
        }
    }
}

fn cargo_cmd() -> Command {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    Command::new(cargo)
}

pub fn run_cargo(root: &Utf8Path, args: &[&str]) -> anyhow::Result<CmdOutput> {
    debug!("Run `cargo {}` in {root}", args.join(" "));

    let output = cargo_cmd()
        .current_dir(root)
        .args(args)
        .output()
        .context("cannot run cargo")?;

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

/// Check if the package is published in the index.
///
/// Unfortunately, the `cargo` cli doesn't provide a way
/// to programmatically detect if a package at a certain version is published.
/// There's `cargo info` but it is a human-focused command with very few
/// compatibility guarantees around its behavior.
/// Therefore, we query the registry index through Cargo's internal source API.
pub async fn is_published(
    index: &mut CargoIndex,
    package: &Package,
    timeout: Duration,
    token: &Option<SecretString>,
) -> anyhow::Result<bool> {
    tokio::time::timeout(timeout, async {
        with_registry_token(index, token, || is_published_cargo(index, package))
    })
    .await?
    .with_context(|| format!("timeout while publishing {}", package.name))
}

fn is_published_cargo(index: &CargoIndex, package: &Package) -> anyhow::Result<bool> {
    let config =
        new_cargo_config(index.cargo_cwd().to_owned()).context("unable to get cargo config")?;
    let source_id = index
        .source_id(&config)
        .with_context(|| format!("can't determine source id for package {}", package.name))?;
    let _lock = config
        .acquire_package_cache_lock(CacheLockMode::DownloadExclusive)
        .context("failed to acquire Cargo package cache lock")?;
    let map = SourceConfigMap::new(&config).context("failed to initialize cargo source map")?;
    let mut source = map
        .load(source_id, &HashSet::default())
        .context("failed to load cargo source")?;
    source.invalidate_cache();

    let mut dependency = Dependency::parse(package.name.as_str(), None, source.source_id())
        .context("failed to build package dependency query")?;
    dependency.lock_version(&package.version);

    let mut published = false;
    loop {
        match source.query(&dependency, QueryKind::RejectedVersions, &mut |_| {
            published = true;
        }) {
            Poll::Ready(Ok(())) => break,
            Poll::Ready(Err(err)) => return none_or_query_err(err),
            Poll::Pending => source
                .block_until_ready()
                .context("failed waiting for registry query to finish")?,
        }
    }

    Ok(published)
}

fn none_or_query_err(err: anyhow::Error) -> anyhow::Result<bool> {
    if err.to_string().contains("failed to fetch") {
        // This may happen with empty registries where metadata cannot be fetched yet.
        Ok(false)
    } else {
        Err(err)
    }
}

fn with_registry_token<T>(
    index: &CargoIndex,
    token: &Option<SecretString>,
    f: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let Some(token) = token else {
        return f();
    };
    let token_env_var_name = index.token_env_var_name()?;

    // Environment variables are process-global, so serialize overrides.
    let _token_env_lock = TOKEN_ENV_LOCK.lock().expect("token env lock poisoned");
    let previous_token = env::var(&token_env_var_name).ok();

    // SAFETY: Access is serialized with a global lock and values are restored before returning.
    unsafe { env::set_var(&token_env_var_name, token.expose_secret()) };
    let result = f();

    if let Some(previous_token) = previous_token {
        // SAFETY: Access is serialized with a global lock and values are restored before returning.
        unsafe { env::set_var(&token_env_var_name, previous_token) };
    } else {
        // SAFETY: Access is serialized with a global lock and values are restored before returning.
        unsafe { env::remove_var(&token_env_var_name) };
    }

    result
}

fn new_cargo_config(cwd: Utf8PathBuf) -> anyhow::Result<GlobalContext> {
    let shell = cargo::core::Shell::new();
    let homedir = homedir(cwd.as_std_path()).context(
        "Cargo couldn't find your home directory. This probably means that $HOME was not set.",
    )?;
    Ok(GlobalContext::new(shell, cwd.into_std_path_buf(), homedir))
}

pub async fn wait_until_published(
    index: &mut CargoIndex,
    package: &Package,
    timeout: Duration,
    token: &Option<SecretString>,
) -> anyhow::Result<()> {
    let now: Instant = Instant::now();
    let sleep_time = Duration::from_secs(2);
    let mut logged = false;

    loop {
        let is_published = is_published(index, package, timeout, token).await?;
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
