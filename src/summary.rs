//! Run summary with atomic counters and final statistics display.
//!
//! [`RunSummary`] accumulates per-image outcomes during a pipeline run and
//! prints a human-readable summary at the end.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Accumulates counts and statistics for a single imgcull pipeline run.
///
/// All numeric counters use [`AtomicUsize`] so they can be incremented from
/// concurrent Tokio tasks without additional synchronisation.  The `best` score
/// and running `score_sum` are protected by a [`Mutex`] because they require a
/// read-modify-write that spans multiple fields.
pub struct RunSummary {
    /// Total number of images submitted to the pipeline.
    pub total: AtomicUsize,
    /// Number of images that received a quality score.
    pub scored: AtomicUsize,
    /// Number of images that received a description.
    pub described: AtomicUsize,
    /// Number of images skipped because a description was already present.
    pub skipped_existing_description: AtomicUsize,
    /// Number of images skipped because the file format is not supported.
    pub skipped_unsupported: AtomicUsize,
    /// Number of images skipped due to an LLM API error.
    pub skipped_llm_error: AtomicUsize,
    /// Number of images skipped because the file could not be read.
    pub skipped_unreadable: AtomicUsize,
    /// The highest-scoring image seen so far: `(filename, score)`.
    pub best: Mutex<Option<(String, f64)>>,
    /// Running sum of all recorded scores, used to compute the average.
    pub score_sum: Mutex<f64>,
}

impl RunSummary {
    /// Create a new [`RunSummary`] with all counters reset to zero.
    pub fn new() -> Self {
        Self {
            total: AtomicUsize::new(0),
            scored: AtomicUsize::new(0),
            described: AtomicUsize::new(0),
            skipped_existing_description: AtomicUsize::new(0),
            skipped_unsupported: AtomicUsize::new(0),
            skipped_llm_error: AtomicUsize::new(0),
            skipped_unreadable: AtomicUsize::new(0),
            best: Mutex::new(None),
            score_sum: Mutex::new(0.0),
        }
    }

    /// Record a successfully computed score for the named image.
    ///
    /// Increments the `scored` counter, adds `score` to the running sum, and
    /// updates `best` if `score` exceeds the current best.
    pub fn record_score(&self, filename: &str, score: f64) {
        self.scored.fetch_add(1, Ordering::Relaxed);
        let mut sum = self.score_sum.lock().unwrap();
        *sum += score;
        let mut best = self.best.lock().unwrap();
        if best.as_ref().is_none_or(|(_, s)| score > *s) {
            *best = Some((filename.to_string(), score));
        }
    }

    /// Print a formatted run summary to stderr.
    ///
    /// Shows totals, scored/described counts with averages, and a warning line
    /// for any skipped images.
    pub fn display(&self) {
        let total = self.total.load(Ordering::Relaxed);
        let scored = self.scored.load(Ordering::Relaxed);
        let described = self.described.load(Ordering::Relaxed);
        let skip_desc = self.skipped_existing_description.load(Ordering::Relaxed);
        let skip_unsup = self.skipped_unsupported.load(Ordering::Relaxed);
        let skip_llm = self.skipped_llm_error.load(Ordering::Relaxed);
        let skip_unread = self.skipped_unreadable.load(Ordering::Relaxed);
        let skipped = skip_unsup + skip_llm + skip_unread;
        let processed = total - skipped;

        let avg = if scored > 0 {
            *self.score_sum.lock().unwrap() / scored as f64
        } else {
            0.0
        };
        let best = self.best.lock().unwrap();

        eprintln!("\nimgcull: {processed}/{total} images processed");
        if scored > 0
            && let Some((name, score)) = best.as_ref()
        {
            eprintln!("  ✓ {scored} scored (avg: {avg:.2}, best: {name} {score:.2})");
        }
        if described > 0 || skip_desc > 0 {
            eprintln!("  ✓ {described} described ({skip_desc} already had descriptions)");
        }
        if skipped > 0 {
            eprintln!(
                "  ⚠ {skipped} skipped ({skip_unsup} unsupported format, {skip_llm} LLM errors, {skip_unread} unreadable)"
            );
        }
    }
}

impl Default for RunSummary {
    fn default() -> Self {
        Self::new()
    }
}
