use tracing_log::LogTracer;
use tracing_subscriber::{filter, layer::SubscriberExt, EnvFilter, FmtSubscriber};

/// Use `info` level by default, but you can customize it with `RUST_LOG` environment variable.
pub fn init(verbose: bool) {
    let subscriber = {
        let env_verbose = std::env::var("RUST_LOG").is_ok() || verbose;
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let event_fmt = tracing_subscriber::fmt::format()
            .pretty()
            .with_source_location(env_verbose)
            .with_target(env_verbose);
        let layer_filter = filter::filter_fn(move |metadata| {
            if env_verbose {
                true
            } else {
                !(metadata.level() == &tracing::Level::INFO && metadata.is_span())
            }
        });

        FmtSubscriber::builder()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .event_format(event_fmt)
            .finish()
            .with(layer_filter)
    };

    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default subscriber failed.");
    LogTracer::init().expect("Failed to initialise log tracer capturing.");
}
