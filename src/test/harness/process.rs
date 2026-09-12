//! Real subprocess tests with explicitly controlled deadline time.

use std::future::Future;
#[cfg(unix)]
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

const PROCESS_TEST_WATCHDOG: Duration = Duration::from_secs(30);

/// Independent real-decompiler acceptance tests share one process-wide budget.
/// Isolate their lifetimes so unrelated tests cannot consume each other's slots.
/// The production permits and limits remain active inside each test.
pub(crate) fn isolate_decompiler_budget() -> std::sync::MutexGuard<'static, ()> {
    static TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // This lock protects no data; a failed test must not poison later cases.
    TESTS.lock().unwrap_or_else(|error| error.into_inner())
}

/// Exercise ordinary production futures and real children with manual Tokio
/// time. OS scheduling must not decide argv, stdin, or admission assertions.
/// Timeout tests explicitly advance time after observing child readiness.
///
/// The blocking watchdog prevents Tokio from auto-advancing while waiting on
/// real I/O, and independently bounds a hung test in wall time. Dropping its
/// sender also releases it on panic/cancellation, so runtime shutdown cannot
/// wait for an abandoned watchdog. Call from a current-thread Tokio test.
pub(crate) async fn with_process_clock<F: Future>(test: F) -> F::Output {
    tokio::time::pause();
    struct ResumeClock;
    impl Drop for ResumeClock {
        fn drop(&mut self) {
            tokio::time::resume();
        }
    }
    let _clock = ResumeClock;
    let (release, waiting) = mpsc::channel::<()>();
    let mut watchdog = tokio::task::spawn_blocking(move || {
        matches!(
            waiting.recv_timeout(PROCESS_TEST_WATCHDOG),
            Err(mpsc::RecvTimeoutError::Timeout)
        )
    });

    tokio::select! {
        result = test => {
            drop(release);
            assert!(
                !watchdog.await.expect("process-test watchdog must not panic"),
                "process test exceeded its 30-second wall-clock watchdog; inspect the child and synchronization"
            );
            result
        }
        expired = &mut watchdog => {
            assert!(expired.expect("process-test watchdog must not panic"));
            panic!("process test exceeded its 30-second wall-clock watchdog; inspect the child and synchronization");
        }
    }
}

/// Observe a child-written readiness marker without advancing its deadline.
/// The enclosing `with_process_clock` provides the wall-clock hang bound.
#[cfg(unix)]
pub(crate) async fn wait_for_process_ready(path: &Path) {
    loop {
        match std::fs::read(path) {
            Ok(bytes) if !bytes.is_empty() => return,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to read child readiness marker: {error}"),
        }
        tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(10)))
            .await
            .expect("readiness polling must not panic");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn process_clock_keeps_real_io_from_consuming_deadline() {
        with_process_clock(async {
            let started = tokio::time::Instant::now();
            let result = tokio::time::timeout(Duration::from_millis(1), async {
                tokio::task::spawn_blocking(|| {
                    // Represent delayed OS I/O independently of Tokio's clock.
                    std::thread::sleep(Duration::from_millis(25));
                    "fixture output"
                })
                .await
                .expect("fixture I/O must complete")
            })
            .await;

            assert_eq!(
                result.expect("real I/O scheduling must not consume the test deadline"),
                "fixture output"
            );
            assert_eq!(started.elapsed(), Duration::ZERO);
        })
        .await;
    }
}
