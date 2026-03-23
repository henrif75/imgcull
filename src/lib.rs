//! imgcull — AI-powered image culling tool using vision LLMs.

pub mod cli;
pub mod config;
pub mod discovery;
pub mod llm;
pub mod pipeline;
pub mod preprocessing;
pub mod retry;
pub mod scoring;
pub mod summary;
pub mod xmp;

use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialise the global tracing subscriber.
///
/// A stderr layer is always installed at the level chosen by `verbose` / `quiet`:
/// - `quiet`   → `error`
/// - `verbose` → `debug`
/// - default   → `warn`
///
/// When `log_file` is provided an additional layer at `debug` level is written
/// to that file using a non-blocking writer so the file handle is
/// `Send + 'static`.
///
/// # Errors
///
/// Returns an error if the log file cannot be created.
pub fn setup_logging(
    verbose: bool,
    quiet: bool,
    log_file: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let level = if quiet {
        "error"
    } else if verbose {
        "debug"
    } else {
        "warn"
    };

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_filter(EnvFilter::new(level));

    if let Some(log_path) = log_file {
        let file = std::fs::File::create(log_path)?;
        let (non_blocking, guard) = tracing_appender::non_blocking(file);
        // Leak the guard so the background thread is never dropped for the
        // lifetime of the process.
        std::mem::forget(guard);

        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_filter(EnvFilter::new("debug"));

        tracing_subscriber::registry()
            .with(stderr_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry().with(stderr_layer).init();
    }

    Ok(())
}
