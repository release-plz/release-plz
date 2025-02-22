use tracing_log::LogTracer;
use tracing_subscriber::{EnvFilter, FmtSubscriber, filter, layer::SubscriberExt};

/// Use `info` level by default, but you can customize it with `RUST_LOG` environment variable.
pub fn init(verbose: bool) {
    let subscriber = {
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let event_fmt = tracing_subscriber::fmt::format()
            .pretty()
            .with_source_location(verbose)
            .with_target(verbose);

        // filters out INFO level span events when verbose mode is disabled
        let layer_filter = filter::filter_fn(move |metadata| {
            if verbose {
                true // show all metadata in verbose mode
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
