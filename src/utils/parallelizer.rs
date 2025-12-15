//! Parallel Processing Utilities
//!
//! Helper functions for processing items in parallel using tokio tasks.

use anyhow::Result;
use futures::future::join_all;
use log::warn;
use std::future::Future;

/// Process a list of items in parallel batches
///
/// # Arguments
///
/// * `items` - The list of items to process
/// * `batch_size` - The number of items to process concurrently
/// * `f` - A function that takes an item and returns a Future
///
/// # Example
///
/// ```rust
/// use crate::utils::parallelizer::process_in_parallel;
///
/// async fn example() {
///     let items = vec![1, 2, 3, 4, 5];
///     process_in_parallel(&items, 2, |item| async move {
///         println!("Processing {}", item);
///         Ok(())
///     }).await.unwrap();
/// }
/// ```
pub async fn process_in_parallel<T, F, Fut>(items: &[T], batch_size: usize, f: F) -> Result<()>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    for batch in items.chunks(batch_size) {
        let mut tasks = Vec::with_capacity(batch.len());

        for item in batch {
            let item = item.clone();
            let f = f.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = f(item).await {
                    warn!("Parallel task failed: {}", e);
                }
            }));
        }

        join_all(tasks).await;
    }

    Ok(())
}
