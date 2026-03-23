use imgcull::retry::retry_with_backoff;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn test_retry_succeeds_on_first_try() {
    tokio::time::pause();
    let result = retry_with_backoff(3, || async { Ok::<_, anyhow::Error>("ok") }).await;
    assert_eq!(result.unwrap(), "ok");
}

#[tokio::test]
async fn test_retry_succeeds_after_failures() {
    tokio::time::pause();
    let attempts = Arc::new(AtomicUsize::new(0));
    let a = attempts.clone();
    let result = retry_with_backoff(3, move || {
        let a = a.clone();
        async move {
            let n = a.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(anyhow::anyhow!("transient error"))
            } else {
                Ok("recovered")
            }
        }
    })
    .await;
    assert_eq!(result.unwrap(), "recovered");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_exhausts_all_attempts() {
    tokio::time::pause();
    let attempts = Arc::new(AtomicUsize::new(0));
    let a = attempts.clone();
    let result: Result<&str, _> = retry_with_backoff(3, move || {
        let a = a.clone();
        async move {
            a.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("persistent error"))
        }
    })
    .await;
    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_zero_attempts_returns_error() {
    tokio::time::pause();
    let result = retry_with_backoff(0, || async { Ok::<_, anyhow::Error>("ok") }).await;
    assert!(result.is_err());
}
