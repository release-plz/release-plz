use tracing::{level_filters::LevelFilter, Level};
use tracing_subscriber::{
    filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

/// Intialize the logging using the tracing crate
///
/// Uses the `INFO` level by default, but you can customize it with `RELEASE_PLZ_LOG` environment
/// variable. If the `RELEASE_PLZ_LOG` environment variable is not set, falls back to the `RUST_LOG`
/// environment variable or the default log level (INFO).
pub fn init(verbosity: LevelFilter) {
    let env_filter = EnvFilter::try_from_env("RELEASE_PLZ_LOG").unwrap_or_else(|_| {
        EnvFilter::builder()
            .with_default_directive(verbosity.into())
            .from_env_lossy()
    });

    // disable spans below WARN level span unless using user has increased verbosity
    let verbose = env_filter
        .max_level_hint()
        .is_some_and(|level| level > Level::INFO);
    let ignore_info_spans = filter_fn(move |metadata| {
        verbose || !metadata.is_span() || metadata.level() < &Level::INFO
    });
    fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_target(verbose)
        .with_file(verbose)
        .with_line_number(verbose)
        .finish()
        .with(ignore_info_spans)
        .init();
}
