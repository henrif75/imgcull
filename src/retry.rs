//! Retry utility with exponential backoff for transient failures.
//!
//! Used by the processing pipeline to handle transient LLM API failures.

use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tracing::warn;

/// Retry an async operation with exponential backoff.
///
/// Delays between retries double each time: 1s, 2s, 4s, ...
///
/// `max_attempts` includes the initial try (e.g., `3` = 1 initial try + 2 retries).
///
/// # Errors
///
/// Returns the last error if all attempts fail.
pub async fn retry_with_backoff<F, Fut, T>(max_attempts: usize, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if max_attempts == 0 {
        anyhow::bail!("max_attempts must be at least 1");
    }

    let mut delay = Duration::from_secs(1);
    let mut last_err = None;

    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                warn!(
                    "Attempt {attempt}/{max_attempts} failed: {e:#}. {}",
                    if attempt < max_attempts {
                        format!("Retrying in {}s...", delay.as_secs())
                    } else {
                        "No more retries.".to_string()
                    }
                );
                last_err = Some(e);
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }
    }

    Err(last_err.unwrap())
}
