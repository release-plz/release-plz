use std::sync::LazyLock;

use tracing_log::LogTracer;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn init() {
    // Only initialize once logs once
    LazyLock::force(&TEST_LOGS);
}

/// Initialize logs if `ENABLE_LOGS` environment variable is set.
/// Use `debug` level by default, but you can customize it with `RUST_LOG` environment variable.
fn _init() {
    if std::env::var("ENABLE_LOGS").is_ok() {
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug,hyper=info"));

        let subscriber = FmtSubscriber::builder()
            .with_env_filter(env_filter)
            .pretty()
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
        LogTracer::init().expect("Failed to initialise log tracer capturing.");
    }
}

static TEST_LOGS: LazyLock<()> = LazyLock::new(_init);
