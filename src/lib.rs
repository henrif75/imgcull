//! imgcull — AI-powered image culling tool using vision LLMs.

pub mod cli;
pub mod config;
pub mod discovery;
pub mod llm;
pub mod pipeline;
pub mod preprocessing;
pub mod report;
pub mod retry;
pub mod scoring;
pub mod summary;
pub mod xmp;

use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Global progress bar used by [`ProgressBarWriter`] to route log lines above
/// the bar.  Set once via [`attach_progress_bar`] when the pipeline starts.
static PROGRESS_BAR: std::sync::OnceLock<indicatif::ProgressBar> = std::sync::OnceLock::new();

/// Register a progress bar so that subsequent log lines (warn/error on stderr)
/// are printed above it via [`indicatif::ProgressBar::println`] instead of
/// being written raw to stderr and corrupting the bar display.
///
/// Must be called after [`setup_logging`] and before the pipeline starts
/// emitting tracing events.  Calling more than once per process has no effect.
pub fn attach_progress_bar(pb: indicatif::ProgressBar) {
    let _ = PROGRESS_BAR.set(pb);
}

/// Initialise the global tracing subscriber.
///
/// A stderr layer is always installed at the level chosen by `verbose` / `quiet`:
/// - `quiet`   → `error`
/// - `verbose` → `debug`
/// - default   → `warn`
///
/// When [`attach_progress_bar`] has been called, the stderr layer routes log
/// lines through the progress bar's `println` method so they appear above the
/// bar without corrupting it.
///
/// When `log_file` is provided an additional file layer is appended to that
/// path at `debug` level using a non-blocking writer.  The file layer always
/// logs at debug regardless of `verbose` / `quiet` — those flags only control
/// stderr output.  The file is opened in append mode so successive runs
/// accumulate in the same log.
///
/// ANSI escape codes that leak from the shared tracing registry span cache
/// (when the stderr layer has ANSI enabled) are stripped by wrapping the file
/// in an [`AnsiStripWriter`].
///
/// # Errors
///
/// Returns an error if the log file cannot be opened.
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

    // Suppress rawler's internal diagnostics on stderr — they are noisy and not
    // actionable by the user (e.g. "Decoder has no preview image support").
    // The file log layer still captures them at debug level for troubleshooting.
    let stderr_filter = format!("{level},rawler=off");
    let stderr_layer = fmt::layer()
        .with_writer(ProgressBarWriterFactory)
        .with_target(false)
        .with_ansi(false)
        .with_filter(EnvFilter::new(stderr_filter));

    if let Some(log_path) = log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        let stripped = AnsiStripWriter(file);
        let (non_blocking, guard) = tracing_appender::non_blocking(stripped);
        // Leak the guard so the background thread is never dropped for the
        // lifetime of the process.
        std::mem::forget(guard);

        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            // Always capture full debug output from all crates.
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

/// Factory that creates a [`ProgressBarWriter`] per tracing event.
///
/// `tracing-subscriber`'s fmt layer calls [`fmt::MakeWriter::make_writer`]
/// once per log event, writes all formatted bytes to the returned writer, then
/// calls [`std::io::Write::flush`].  Buffering in the per-event writer and
/// flushing in one shot avoids multiple [`indicatif::ProgressBar::println`]
/// calls (which would each re-render the bar and leave ghost lines).
struct ProgressBarWriterFactory;

/// Per-event buffered writer.  Accumulates bytes during the event and emits
/// them as a single [`indicatif::ProgressBar::println`] call on flush.
struct ProgressBarWriter(Vec<u8>);

impl std::io::Write for ProgressBarWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(pb) = PROGRESS_BAR.get() {
            let s = String::from_utf8_lossy(&self.0);
            let line = s.trim_end_matches('\n');
            if !line.is_empty() {
                pb.println(line);
            }
        } else {
            std::io::stderr().write_all(&self.0)?;
            std::io::stderr().flush()?;
        }
        self.0.clear();
        Ok(())
    }
}

impl Drop for ProgressBarWriter {
    fn drop(&mut self) {
        // Flush any remaining buffered bytes that were not yet emitted.  This
        // ensures output is not silently dropped if the fmt layer does not call
        // flush() explicitly (behaviour varies across tracing-subscriber versions).
        use std::io::Write as _;
        let _ = self.flush();
    }
}

impl<'a> fmt::MakeWriter<'a> for ProgressBarWriterFactory {
    type Writer = ProgressBarWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ProgressBarWriter(Vec::new())
    }
}

/// A writer wrapper that strips ANSI escape sequences before forwarding to
/// the inner writer.
///
/// When `tracing_subscriber`'s stderr layer formats spans with ANSI enabled,
/// the registry caches those formatted fields.  A second layer (the file
/// layer) sharing the same registry reads the cached, ANSI-contaminated span
/// data even when configured with `.with_ansi(false)`.  Wrapping the file
/// writer in this struct ensures all ANSI escapes are removed before writing.
struct AnsiStripWriter<W>(W);

impl<W: std::io::Write> std::io::Write for AnsiStripWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = buf.len();
        let stripped = strip_ansi_bytes(buf);
        self.0.write_all(&stripped)?;
        // Report the original length so the caller knows all input was consumed.
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Strip ANSI CSI escape sequences (`\x1b[...m`) from a byte slice.
fn strip_ansi_bytes(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x1b && input.get(i + 1) == Some(&b'[') {
            // Skip past the CSI sequence until we hit the terminating byte
            // (ASCII 0x40–0x7E, which includes 'm').
            i += 2;
            while i < input.len() {
                let b = input[i];
                i += 1;
                if (0x40..=0x7E).contains(&b) {
                    break;
                }
            }
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_sequences() {
        let input = b"\x1b[3mfield_name\x1b[0m\x1b[2m=\x1b[0mvalue";
        let result = strip_ansi_bytes(input);
        assert_eq!(result, b"field_name=value");
    }

    #[test]
    fn strip_ansi_passes_through_clean_text() {
        let input = b"no escape codes here";
        let result = strip_ansi_bytes(input);
        assert_eq!(result, input);
    }
}
