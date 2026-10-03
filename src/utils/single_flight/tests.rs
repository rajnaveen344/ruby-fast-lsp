use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[tokio::test]
async fn concurrent_waiters_share_exactly_one_producer() {
    let cache = SingleFlightCache::<String, usize>::default();
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        tasks.push(tokio::spawn(async move {
            cache
                .get_or_try_init("core-3.3".to_string(), || async move {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(42)
                })
                .await
                .unwrap()
        }));
    }
    for task in tasks {
        assert_eq!(*task.await.unwrap(), 42);
    }
    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);
    let stats = cache.snapshot();
    assert_eq!(stats.get(SingleFlightStat::Lookups), 16);
    assert_eq!(stats.get(SingleFlightStat::Misses), 1);
    assert_eq!(stats.get(SingleFlightStat::Producers), 1);
    assert_eq!(
        stats.get(SingleFlightStat::Hits) + stats.get(SingleFlightStat::JoinedFlights),
        15
    );
}

#[tokio::test]
async fn failed_flight_wakes_waiters_and_later_generation_retries() {
    let cache = SingleFlightCache::<String, usize>::default();
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        tasks.push(tokio::spawn(async move {
            cache
                .get_or_try_init("broken".to_string(), || async move {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Err("broken input".to_string())
                })
                .await
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap_err(), "broken input");
    }
    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert!(cache.is_empty());

    let recovered = cache
        .get_or_try_init("broken".to_string(), || async { Ok(7) })
        .await
        .unwrap();
    assert_eq!(*recovered, 7);
    let stats = cache.snapshot();
    assert_eq!(stats.get(SingleFlightStat::Lookups), 9);
    assert_eq!(stats.get(SingleFlightStat::Misses), 2);
    assert_eq!(stats.get(SingleFlightStat::Producers), 2);
    assert_eq!(stats.get(SingleFlightStat::Failures), 1);
}

#[tokio::test]
async fn bounded_cache_evicts_oldest_completed_values_by_weight() {
    let cache = BoundedSingleFlightCache::<String, Vec<u8>>::new(3, 10, |value| value.len() as u64);

    cache
        .get_or_try_init("a".to_string(), || async { Ok(vec![0; 6]) })
        .await
        .unwrap();
    cache
        .get_or_try_init("b".to_string(), || async { Ok(vec![0; 6]) })
        .await
        .unwrap();

    assert_eq!(cache.len(), 1);
    assert_eq!(cache.retained_weight(), 6);
    assert!(!cache.contains_key(&"a".to_string()));
    assert!(cache.contains_key(&"b".to_string()));
    assert_eq!(cache.snapshot().get(SingleFlightStat::Evictions), 1);
}

#[tokio::test]
async fn overweight_value_serves_current_waiters_without_being_retained() {
    let cache = BoundedSingleFlightCache::<String, Vec<u8>>::new(2, 4, |value| value.len() as u64);
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        tasks.push(tokio::spawn(async move {
            cache
                .get_or_try_init("large".to_string(), || async move {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(vec![1; 8])
                })
                .await
                .unwrap()
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().len(), 8);
    }

    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert!(cache.is_empty());
    assert_eq!(cache.retained_weight(), 0);
}

#[tokio::test]
async fn ephemeral_cache_coalesces_one_flight_without_retaining_completed_values() {
    let cache = BoundedSingleFlightCache::<String, usize>::ephemeral(|_| 1);
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        tasks.push(tokio::spawn(async move {
            cache
                .get_or_try_init("shared".to_string(), || async move {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok(42)
                })
                .await
                .unwrap()
        }));
    }
    for task in tasks {
        assert_eq!(*task.await.unwrap(), 42);
    }
    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert!(cache.is_empty());
    assert_eq!(cache.retained_weight(), 0);

    let producer_calls_again = producer_calls.clone();
    let value = cache
        .get_or_try_init("shared".to_string(), || async move {
            producer_calls_again.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        })
        .await
        .unwrap();
    assert_eq!(*value, 42);
    assert_eq!(producer_calls.load(Ordering::SeqCst), 2);
    assert!(cache.is_empty());
}

#[tokio::test]
async fn bounded_cache_does_not_evict_in_flight_producers() {
    let cache = BoundedSingleFlightCache::<String, usize>::new(1, 1, |_| 1);
    let producer_started = Arc::new(tokio::sync::Notify::new());
    let release_producer = Arc::new(tokio::sync::Notify::new());
    let first_task = {
        let cache = cache.clone();
        let producer_started = producer_started.clone();
        let release_producer = release_producer.clone();
        tokio::spawn(async move {
            cache
                .get_or_try_init("first".to_string(), || async move {
                    producer_started.notify_one();
                    release_producer.notified().await;
                    Ok(1)
                })
                .await
                .unwrap()
        })
    };
    producer_started.notified().await;

    cache
        .get_or_try_init("second".to_string(), || async { Ok(2) })
        .await
        .unwrap();
    assert!(cache.contains_key(&"first".to_string()));

    release_producer.notify_one();
    assert_eq!(*first_task.await.unwrap(), 1);
    assert_eq!(cache.len(), 1);
    assert!(cache.retained_weight() <= 1);
}

#[tokio::test]
async fn cancelling_initiating_waiter_does_not_cancel_shared_producer() {
    let cache = BoundedSingleFlightCache::<String, usize>::new(1, 1, |_| 1);
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let producer_started = Arc::new(tokio::sync::Notify::new());
    let release_producer = Arc::new(tokio::sync::Semaphore::new(0));

    let first_waiter = {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        let producer_started = producer_started.clone();
        let release_producer = release_producer.clone();
        tokio::spawn(async move {
            cache
                .get_or_try_init("shared".to_string(), || async move {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    producer_started.notify_one();
                    let _permit = release_producer.acquire().await.unwrap();
                    Ok(42)
                })
                .await
        })
    };
    producer_started.notified().await;

    let second_waiter = {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        let producer_started = producer_started.clone();
        let release_producer = release_producer.clone();
        tokio::spawn(async move {
            cache
                .get_or_try_init("shared".to_string(), || async move {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    producer_started.notify_one();
                    let _permit = release_producer.acquire().await.unwrap();
                    Ok(42)
                })
                .await
        })
    };

    while cache.snapshot().get(SingleFlightStat::JoinedFlights) == 0 {
        tokio::task::yield_now().await;
    }
    first_waiter.abort();
    assert!(first_waiter.await.unwrap_err().is_cancelled());
    release_producer.add_permits(2);

    let result = tokio::time::timeout(Duration::from_secs(1), second_waiter)
        .await
        .expect("shared producer should complete after the initiating waiter is cancelled")
        .unwrap()
        .unwrap();
    assert_eq!(*result, 42);
    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.snapshot().get(SingleFlightStat::Producers), 1);
}

#[test]
fn blocking_bounded_cache_coalesces_and_retains_exact_products() {
    let cache = BlockingBoundedSingleFlightCache::<String, usize, String>::new(2, 2, |_| 1);
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();

    let first = {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        std::thread::spawn(move || {
            cache
                .get_or_try_init("shared".to_string(), || {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(42)
                })
                .unwrap()
        })
    };
    started_rx.recv().unwrap();

    let second = {
        let cache = cache.clone();
        let producer_calls = producer_calls.clone();
        std::thread::spawn(move || {
            cache
                .get_or_try_init("shared".to_string(), || {
                    producer_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(99)
                })
                .unwrap()
        })
    };
    while cache.snapshot().get(SingleFlightStat::JoinedFlights) == 0 {
        std::thread::yield_now();
    }
    release_tx.send(()).unwrap();

    assert_eq!(*first.join().unwrap(), 42);
    assert_eq!(*second.join().unwrap(), 42);
    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);

    let retained = cache
        .get_or_try_init("shared".to_string(), || {
            producer_calls.fetch_add(1, Ordering::SeqCst);
            Ok(7)
        })
        .unwrap();
    assert_eq!(*retained, 42);
    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.retained_weight(), 1);
    assert_eq!(cache.snapshot().get(SingleFlightStat::Producers), 1);
    assert_eq!(cache.snapshot().get(SingleFlightStat::JoinedFlights), 1);
    assert_eq!(cache.snapshot().get(SingleFlightStat::Hits), 1);
}

#[test]
fn blocking_bounded_cache_retries_failed_products_and_evicts_by_weight() {
    let cache = BlockingBoundedSingleFlightCache::<String, Vec<u8>, String>::new(2, 4, |value| {
        value.len() as u64
    });
    assert_eq!(
        cache
            .get_or_try_init("retry".to_string(), || Err("broken".to_string()))
            .unwrap_err(),
        "broken"
    );
    assert!(cache.is_empty());

    cache
        .get_or_try_init("retry".to_string(), || Ok(vec![1; 3]))
        .unwrap();
    cache
        .get_or_try_init("newer".to_string(), || Ok(vec![2; 3]))
        .unwrap();

    assert!(!cache.contains_key(&"retry".to_string()));
    assert!(cache.contains_key(&"newer".to_string()));
    assert_eq!(cache.retained_weight(), 3);
    let snapshot = cache.snapshot();
    assert_eq!(snapshot.get(SingleFlightStat::Failures), 1);
    assert_eq!(snapshot.get(SingleFlightStat::Producers), 3);
    assert_eq!(snapshot.get(SingleFlightStat::Evictions), 1);
}

#[test]
fn blocking_bounded_cache_wakes_waiters_and_retries_after_producer_panic() {
    let cache = BlockingBoundedSingleFlightCache::<String, usize, String>::new(1, 1, |_| 1);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let producer = {
        let cache = cache.clone();
        std::thread::spawn(move || {
            cache.get_or_try_init("panic".to_string(), || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                panic!("fixture producer panic")
            })
        })
    };
    started_rx.recv().unwrap();
    let waiter = {
        let cache = cache.clone();
        std::thread::spawn(move || cache.get_or_try_init("panic".to_string(), || Ok(99)))
    };
    while cache.snapshot().get(SingleFlightStat::JoinedFlights) == 0 {
        std::thread::yield_now();
    }
    release_tx.send(()).unwrap();

    assert!(producer.join().is_err());
    assert!(waiter.join().is_err());
    assert!(cache.is_empty());
    assert_eq!(cache.snapshot().get(SingleFlightStat::Failures), 1);
    assert_eq!(
        *cache
            .get_or_try_init("panic".to_string(), || Ok(42))
            .unwrap(),
        42
    );
    assert_eq!(cache.snapshot().get(SingleFlightStat::Producers), 2);
}
