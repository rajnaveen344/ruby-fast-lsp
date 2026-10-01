use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, OnceCell};

type SharedResult<V> = Result<Arc<V>, Arc<String>>;

#[derive(Debug)]
struct FlightState<V> {
    result: OnceCell<SharedResult<V>>,
    ready: Notify,
}

impl<V> FlightState<V> {
    fn new() -> Self {
        Self {
            result: OnceCell::new(),
            ready: Notify::new(),
        }
    }

    fn get(&self) -> Option<&SharedResult<V>> {
        self.result.get()
    }

    async fn wait(&self) -> SharedResult<V> {
        loop {
            // Register before checking the cell so completion between the check
            // and the await cannot be lost.
            let ready = self.ready.notified();
            if let Some(result) = self.result.get() {
                return result.clone();
            }
            ready.await;
        }
    }

    fn complete(&self, result: SharedResult<V>) {
        assert!(
            self.result.set(result).is_ok(),
            "INVARIANT VIOLATED: a single-flight producer completed the same key more than once. This is a bug because exactly one producer owns each flight. Fix: inspect producer admission and completion ownership."
        );
        self.ready.notify_waiters();
    }
}

type FlightCell<V> = Arc<FlightState<V>>;

#[derive(Debug)]
pub struct SingleFlightCache<K, V> {
    entries: Arc<Mutex<HashMap<K, FlightCell<V>>>>,
    counters: Arc<SingleFlightCounters>,
}

#[derive(Debug, Default)]
struct SingleFlightCounters {
    lookups: AtomicU64,
    hits: AtomicU64,
    joined_flights: AtomicU64,
    misses: AtomicU64,
    producers: AtomicU64,
    failures: AtomicU64,
    evictions: AtomicU64,
    producer_wall_ns: AtomicU64,
    producer_max_wall_ns: AtomicU64,
    consumer_wait_wall_ns: AtomicU64,
    consumer_max_wait_wall_ns: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SingleFlightSnapshot {
    pub entries: usize,
    pub lookups: u64,
    pub hits: u64,
    pub joined_flights: u64,
    pub misses: u64,
    pub producers: u64,
    pub failures: u64,
    pub evictions: u64,
    pub producer_wall_ns: u64,
    pub producer_max_wall_ns: u64,
    pub consumer_wait_wall_ns: u64,
    pub consumer_max_wait_wall_ns: u64,
}

impl<K, V> Clone for SingleFlightCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            counters: self.counters.clone(),
        }
    }
}

impl<K, V> Default for SingleFlightCache<K, V> {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(SingleFlightCounters::default()),
        }
    }
}

impl<K, V> SingleFlightCache<K, V>
where
    K: Clone + Eq + Hash + Send + 'static,
    V: Send + Sync + 'static,
{
    pub async fn get_or_try_init<F, Fut>(&self, key: K, producer: F) -> Result<Arc<V>, String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<V, String>> + Send + 'static,
    {
        self.counters.lookups.fetch_add(1, Ordering::Relaxed);
        let (cell, owns_producer) = {
            use std::collections::hash_map::Entry;

            let mut entries = self.entries.lock();
            match entries.entry(key.clone()) {
                Entry::Occupied(entry) => {
                    if entry.get().get().is_some() {
                        self.counters.hits.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.counters.joined_flights.fetch_add(1, Ordering::Relaxed);
                    }
                    (entry.get().clone(), false)
                }
                Entry::Vacant(entry) => {
                    self.counters.misses.fetch_add(1, Ordering::Relaxed);
                    (entry.insert(Arc::new(FlightState::new())).clone(), true)
                }
            }
        };
        if owns_producer {
            self.counters.producers.fetch_add(1, Ordering::Relaxed);
            let producer_started = Instant::now();
            let producer_task = tokio::spawn(producer());
            let producer_cell = cell.clone();
            let producer_key = key.clone();
            let entries = self.entries.clone();
            let counters = self.counters.clone();
            tokio::spawn(async move {
                let result = match producer_task.await {
                    Ok(result) => result.map(Arc::new).map_err(Arc::new),
                    Err(error) => Err(Arc::new(format!(
                        "single-flight producer task failed: {error}"
                    ))),
                };
                counters.record_producer_wall(producer_started.elapsed());
                if result.is_err() {
                    counters.failures.fetch_add(1, Ordering::Relaxed);
                }
                producer_cell.complete(result);
                if producer_cell.get().is_some_and(Result::is_err) {
                    let mut entries = entries.lock();
                    if entries
                        .get(&producer_key)
                        .is_some_and(|current| Arc::ptr_eq(current, &producer_cell))
                    {
                        entries.remove(&producer_key);
                    }
                }
            });
        }
        let wait_started = Instant::now();
        let result = cell.wait().await;
        self.counters
            .record_consumer_wait_wall(wait_started.elapsed());
        result.map_err(|error| error.as_ref().clone())
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    pub fn snapshot(&self) -> SingleFlightSnapshot {
        SingleFlightSnapshot {
            entries: self.len(),
            lookups: self.counters.lookups.load(Ordering::Relaxed),
            hits: self.counters.hits.load(Ordering::Relaxed),
            joined_flights: self.counters.joined_flights.load(Ordering::Relaxed),
            misses: self.counters.misses.load(Ordering::Relaxed),
            producers: self.counters.producers.load(Ordering::Relaxed),
            failures: self.counters.failures.load(Ordering::Relaxed),
            evictions: self.counters.evictions.load(Ordering::Relaxed),
            producer_wall_ns: self.counters.producer_wall_ns.load(Ordering::Relaxed),
            producer_max_wall_ns: self.counters.producer_max_wall_ns.load(Ordering::Relaxed),
            consumer_wait_wall_ns: self.counters.consumer_wait_wall_ns.load(Ordering::Relaxed),
            consumer_max_wait_wall_ns: self
                .counters
                .consumer_max_wait_wall_ns
                .load(Ordering::Relaxed),
        }
    }
}

struct BoundedFlightEntry<V> {
    cell: FlightCell<V>,
    generation: u64,
}

pub struct BoundedSingleFlightCache<K, V> {
    entries: Arc<Mutex<HashMap<K, BoundedFlightEntry<V>>>>,
    counters: Arc<SingleFlightCounters>,
    next_generation: Arc<AtomicU64>,
    max_entries: usize,
    max_weight: u64,
    weight: Arc<dyn Fn(&V) -> u64 + Send + Sync>,
}

impl<K, V> Clone for BoundedSingleFlightCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            counters: self.counters.clone(),
            next_generation: self.next_generation.clone(),
            max_entries: self.max_entries,
            max_weight: self.max_weight,
            weight: self.weight.clone(),
        }
    }
}

impl<K, V> BoundedSingleFlightCache<K, V>
where
    K: Clone + Eq + Hash + Send + 'static,
    V: Send + Sync + 'static,
{
    pub fn ephemeral(weight: impl Fn(&V) -> u64 + Send + Sync + 'static) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(SingleFlightCounters::default()),
            next_generation: Arc::new(AtomicU64::new(0)),
            max_entries: 0,
            max_weight: 0,
            weight: Arc::new(weight),
        }
    }

    pub fn new(
        max_entries: usize,
        max_weight: u64,
        weight: impl Fn(&V) -> u64 + Send + Sync + 'static,
    ) -> Self {
        assert!(
            max_entries > 0,
            "INVARIANT VIOLATED: bounded single-flight cache has a zero entry limit. This is a bug because a zero-retention product should use an explicit ephemeral flight instead of pretending to be a cache. Fix: configure at least one retained entry."
        );
        assert!(
            max_weight > 0,
            "INVARIANT VIOLATED: bounded single-flight cache has a zero weight limit. This is a bug because every retained product must have a positive resource budget. Fix: configure a measured positive byte/weight budget."
        );
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(SingleFlightCounters::default()),
            next_generation: Arc::new(AtomicU64::new(0)),
            max_entries,
            max_weight,
            weight: Arc::new(weight),
        }
    }

    pub async fn get_or_try_init<F, Fut>(&self, key: K, producer: F) -> Result<Arc<V>, String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<V, String>> + Send + 'static,
    {
        self.counters.lookups.fetch_add(1, Ordering::Relaxed);
        let (cell, owns_producer) = {
            use std::collections::hash_map::Entry;

            let mut entries = self.entries.lock();
            match entries.entry(key.clone()) {
                Entry::Occupied(entry) => {
                    if entry.get().cell.get().is_some() {
                        self.counters.hits.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.counters.joined_flights.fetch_add(1, Ordering::Relaxed);
                    }
                    (entry.get().cell.clone(), false)
                }
                Entry::Vacant(entry) => {
                    self.counters.misses.fetch_add(1, Ordering::Relaxed);
                    let generation = self
                        .next_generation
                        .fetch_add(1, Ordering::AcqRel)
                        .checked_add(1)
                        .expect(
                            "INVARIANT VIOLATED: bounded single-flight cache generation overflowed. This is a bug because one process cannot create 2^64 cache entries. Fix: inspect the cache key/invalidation loop.",
                        );
                    (
                        entry
                            .insert(BoundedFlightEntry {
                                cell: Arc::new(FlightState::new()),
                                generation,
                            })
                            .cell
                            .clone(),
                        true,
                    )
                }
            }
        };
        if owns_producer {
            self.counters.producers.fetch_add(1, Ordering::Relaxed);
            let producer_started = Instant::now();
            let producer_task = tokio::spawn(producer());
            let producer_cell = cell.clone();
            let producer_key = key.clone();
            let cache = self.clone();
            tokio::spawn(async move {
                let result = match producer_task.await {
                    Ok(result) => result.map(Arc::new).map_err(Arc::new),
                    Err(error) => Err(Arc::new(format!(
                        "single-flight producer task failed: {error}"
                    ))),
                };
                cache
                    .counters
                    .record_producer_wall(producer_started.elapsed());
                if result.is_err() {
                    cache.counters.failures.fetch_add(1, Ordering::Relaxed);
                }
                producer_cell.complete(result);
                if producer_cell.get().is_some_and(Result::is_err) {
                    let mut entries = cache.entries.lock();
                    if entries
                        .get(&producer_key)
                        .is_some_and(|current| Arc::ptr_eq(&current.cell, &producer_cell))
                    {
                        entries.remove(&producer_key);
                    }
                } else {
                    cache.evict_completed_to_limits();
                }
            });
        }
        let wait_started = Instant::now();
        let result = cell.wait().await;
        self.counters
            .record_consumer_wait_wall(wait_started.elapsed());
        result.map_err(|error| error.as_ref().clone())
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.lock().contains_key(key)
    }

    pub fn retained_weight(&self) -> u64 {
        let entries = self.entries.lock();
        completed_weight(&entries, self.weight.as_ref())
    }

    pub fn snapshot(&self) -> SingleFlightSnapshot {
        SingleFlightSnapshot {
            entries: self.len(),
            lookups: self.counters.lookups.load(Ordering::Relaxed),
            hits: self.counters.hits.load(Ordering::Relaxed),
            joined_flights: self.counters.joined_flights.load(Ordering::Relaxed),
            misses: self.counters.misses.load(Ordering::Relaxed),
            producers: self.counters.producers.load(Ordering::Relaxed),
            failures: self.counters.failures.load(Ordering::Relaxed),
            evictions: self.counters.evictions.load(Ordering::Relaxed),
            producer_wall_ns: self.counters.producer_wall_ns.load(Ordering::Relaxed),
            producer_max_wall_ns: self.counters.producer_max_wall_ns.load(Ordering::Relaxed),
            consumer_wait_wall_ns: self.counters.consumer_wait_wall_ns.load(Ordering::Relaxed),
            consumer_max_wait_wall_ns: self
                .counters
                .consumer_max_wait_wall_ns
                .load(Ordering::Relaxed),
        }
    }

    fn evict_completed_to_limits(&self) {
        let mut entries = self.entries.lock();
        loop {
            let completed_entries = entries
                .values()
                .filter(|entry| matches!(entry.cell.get(), Some(Ok(_))))
                .count();
            let retained_weight = completed_weight(&entries, self.weight.as_ref());
            if completed_entries <= self.max_entries && retained_weight <= self.max_weight {
                break;
            }

            let Some(oldest_key) = entries
                .iter()
                .filter(|(_, entry)| matches!(entry.cell.get(), Some(Ok(_))))
                .min_by_key(|(_, entry)| entry.generation)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            entries.remove(&oldest_key);
            self.counters.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }
}

enum BlockingFlightCompletion<V, E> {
    Pending,
    Completed(Result<Arc<V>, Arc<E>>),
    ProducerPanicked,
}

struct BlockingFlightState<V, E> {
    completion: Mutex<BlockingFlightCompletion<V, E>>,
    ready: Condvar,
}

impl<V, E> BlockingFlightState<V, E> {
    fn new() -> Self {
        Self {
            completion: Mutex::new(BlockingFlightCompletion::Pending),
            ready: Condvar::new(),
        }
    }

    fn is_completed_successfully(&self) -> bool {
        matches!(
            &*self.completion.lock(),
            BlockingFlightCompletion::Completed(Ok(_))
        )
    }

    fn complete(&self, result: Result<Arc<V>, Arc<E>>) {
        let mut completion = self.completion.lock();
        assert!(
            matches!(*completion, BlockingFlightCompletion::Pending),
            "INVARIANT VIOLATED: a blocking single-flight producer completed the same key more than once. This is a bug because exactly one blocking worker owns each flight. Fix: inspect blocking producer admission and completion ownership."
        );
        *completion = BlockingFlightCompletion::Completed(result);
        self.ready.notify_all();
    }

    fn complete_panicked(&self) {
        let mut completion = self.completion.lock();
        assert!(
            matches!(*completion, BlockingFlightCompletion::Pending),
            "INVARIANT VIOLATED: a panicked blocking single-flight producer had already completed. This is a bug because one producer cannot have two terminal states. Fix: inspect panic and completion ownership."
        );
        *completion = BlockingFlightCompletion::ProducerPanicked;
        self.ready.notify_all();
    }
}

impl<V, E> BlockingFlightState<V, E>
where
    E: Clone,
{
    fn wait(&self) -> Result<Arc<V>, E> {
        let mut completion = self.completion.lock();
        loop {
            match &*completion {
                BlockingFlightCompletion::Pending => self.ready.wait(&mut completion),
                BlockingFlightCompletion::Completed(result) => {
                    return result.clone().map_err(|error| error.as_ref().clone());
                }
                BlockingFlightCompletion::ProducerPanicked => {
                    panic!(
                        "INVARIANT VIOLATED: a blocking single-flight consumer observed a panicked producer. This is a bug because the requested immutable product was not constructed. Fix: inspect the producer panic reported by the owning worker."
                    );
                }
            }
        }
    }
}

struct BlockingBoundedFlightEntry<V, E> {
    cell: Arc<BlockingFlightState<V, E>>,
    generation: u64,
}

/// Coalesces immutable work that must execute synchronously inside an already
/// admitted blocking or Rayon worker. Completed products are retained under an
/// exact entry/weight bound; failures are removed so the next request retries.
pub struct BlockingBoundedSingleFlightCache<K, V, E> {
    entries: Arc<Mutex<HashMap<K, BlockingBoundedFlightEntry<V, E>>>>,
    counters: Arc<SingleFlightCounters>,
    next_generation: Arc<AtomicU64>,
    max_entries: usize,
    max_weight: u64,
    weight: Arc<dyn Fn(&V) -> u64 + Send + Sync>,
}

impl<K, V, E> Clone for BlockingBoundedSingleFlightCache<K, V, E> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            counters: self.counters.clone(),
            next_generation: self.next_generation.clone(),
            max_entries: self.max_entries,
            max_weight: self.max_weight,
            weight: self.weight.clone(),
        }
    }
}

impl<K, V, E> BlockingBoundedSingleFlightCache<K, V, E>
where
    K: Clone + Eq + Hash + Send + 'static,
    V: Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    pub fn new(
        max_entries: usize,
        max_weight: u64,
        weight: impl Fn(&V) -> u64 + Send + Sync + 'static,
    ) -> Self {
        assert!(
            max_entries > 0,
            "INVARIANT VIOLATED: blocking single-flight cache has a zero entry limit. This is a bug because completed synchronous products need an explicit retention policy. Fix: configure at least one retained entry."
        );
        assert!(
            max_weight > 0,
            "INVARIANT VIOLATED: blocking single-flight cache has a zero weight limit. This is a bug because retained synchronous products must have a positive resource bound. Fix: configure a measured positive weight budget."
        );
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(SingleFlightCounters::default()),
            next_generation: Arc::new(AtomicU64::new(0)),
            max_entries,
            max_weight,
            weight: Arc::new(weight),
        }
    }

    pub fn get_or_try_init<F>(&self, key: K, producer: F) -> Result<Arc<V>, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        self.counters.lookups.fetch_add(1, Ordering::Relaxed);
        let (cell, owns_producer) = {
            use std::collections::hash_map::Entry;

            let mut entries = self.entries.lock();
            match entries.entry(key.clone()) {
                Entry::Occupied(entry) => {
                    if entry.get().cell.is_completed_successfully() {
                        self.counters.hits.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.counters.joined_flights.fetch_add(1, Ordering::Relaxed);
                    }
                    (entry.get().cell.clone(), false)
                }
                Entry::Vacant(entry) => {
                    self.counters.misses.fetch_add(1, Ordering::Relaxed);
                    let generation = self
                        .next_generation
                        .fetch_add(1, Ordering::AcqRel)
                        .checked_add(1)
                        .expect(
                            "INVARIANT VIOLATED: blocking single-flight generation overflowed. This is a bug because one process cannot construct 2^64 immutable products. Fix: inspect cache invalidation and key creation.",
                        );
                    let cell = Arc::new(BlockingFlightState::new());
                    entry.insert(BlockingBoundedFlightEntry {
                        cell: cell.clone(),
                        generation,
                    });
                    (cell, true)
                }
            }
        };

        if owns_producer {
            self.counters.producers.fetch_add(1, Ordering::Relaxed);
            let producer_started = Instant::now();
            match catch_unwind(AssertUnwindSafe(producer)) {
                Ok(result) => {
                    self.counters
                        .record_producer_wall(producer_started.elapsed());
                    let shared = result.map(Arc::new).map_err(Arc::new);
                    if shared.is_err() {
                        self.counters.failures.fetch_add(1, Ordering::Relaxed);
                        self.remove_if_owned(&key, &cell);
                    }
                    cell.complete(shared);
                    self.evict_completed_to_limits();
                }
                Err(payload) => {
                    self.counters
                        .record_producer_wall(producer_started.elapsed());
                    self.counters.failures.fetch_add(1, Ordering::Relaxed);
                    self.remove_if_owned(&key, &cell);
                    cell.complete_panicked();
                    resume_unwind(payload);
                }
            }
        }

        let wait_started = Instant::now();
        let result = cell.wait();
        self.counters
            .record_consumer_wait_wall(wait_started.elapsed());
        result
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.lock().contains_key(key)
    }

    pub fn retained_weight(&self) -> u64 {
        let entries = self.entries.lock();
        blocking_completed_weight(&entries, self.weight.as_ref())
    }

    pub fn snapshot(&self) -> SingleFlightSnapshot {
        SingleFlightSnapshot {
            entries: self.len(),
            lookups: self.counters.lookups.load(Ordering::Relaxed),
            hits: self.counters.hits.load(Ordering::Relaxed),
            joined_flights: self.counters.joined_flights.load(Ordering::Relaxed),
            misses: self.counters.misses.load(Ordering::Relaxed),
            producers: self.counters.producers.load(Ordering::Relaxed),
            failures: self.counters.failures.load(Ordering::Relaxed),
            evictions: self.counters.evictions.load(Ordering::Relaxed),
            producer_wall_ns: self.counters.producer_wall_ns.load(Ordering::Relaxed),
            producer_max_wall_ns: self.counters.producer_max_wall_ns.load(Ordering::Relaxed),
            consumer_wait_wall_ns: self.counters.consumer_wait_wall_ns.load(Ordering::Relaxed),
            consumer_max_wait_wall_ns: self
                .counters
                .consumer_max_wait_wall_ns
                .load(Ordering::Relaxed),
        }
    }

    fn remove_if_owned(&self, key: &K, cell: &Arc<BlockingFlightState<V, E>>) {
        let mut entries = self.entries.lock();
        if entries
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(&current.cell, cell))
        {
            entries.remove(key);
        }
    }

    fn evict_completed_to_limits(&self) {
        let mut entries = self.entries.lock();
        loop {
            let completed_entries = entries
                .values()
                .filter(|entry| entry.cell.is_completed_successfully())
                .count();
            let retained_weight = blocking_completed_weight(&entries, self.weight.as_ref());
            if completed_entries <= self.max_entries && retained_weight <= self.max_weight {
                break;
            }
            let Some(oldest_key) = entries
                .iter()
                .filter(|(_, entry)| entry.cell.is_completed_successfully())
                .min_by_key(|(_, entry)| entry.generation)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            entries.remove(&oldest_key);
            self.counters.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl SingleFlightCounters {
    fn record_producer_wall(&self, elapsed: Duration) {
        record_duration(&self.producer_wall_ns, &self.producer_max_wall_ns, elapsed);
    }

    fn record_consumer_wait_wall(&self, elapsed: Duration) {
        record_duration(
            &self.consumer_wait_wall_ns,
            &self.consumer_max_wait_wall_ns,
            elapsed,
        );
    }
}

fn record_duration(total: &AtomicU64, maximum: &AtomicU64, elapsed: Duration) {
    let nanoseconds = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    let _ = total.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(nanoseconds))
    });
    maximum.fetch_max(nanoseconds, Ordering::Relaxed);
}

fn completed_weight<K, V>(
    entries: &HashMap<K, BoundedFlightEntry<V>>,
    weight: &(dyn Fn(&V) -> u64 + Send + Sync),
) -> u64 {
    entries
        .values()
        .filter_map(|entry| match entry.cell.get() {
            Some(Ok(value)) => Some(weight(value)),
            Some(Err(_)) | None => None,
        })
        .try_fold(0u64, |total, value| total.checked_add(value))
        .expect(
            "INVARIANT VIOLATED: bounded single-flight cache retained weight overflowed u64. This is a bug because the configured process memory budget is far below u64::MAX. Fix: validate product weight accounting before retention.",
        )
}

fn blocking_completed_weight<K, V, E>(
    entries: &HashMap<K, BlockingBoundedFlightEntry<V, E>>,
    weight: &(dyn Fn(&V) -> u64 + Send + Sync),
) -> u64 {
    entries
        .values()
        .filter_map(|entry| match &*entry.cell.completion.lock() {
            BlockingFlightCompletion::Completed(Ok(value)) => Some(weight(value)),
            BlockingFlightCompletion::Pending
            | BlockingFlightCompletion::Completed(Err(_))
            | BlockingFlightCompletion::ProducerPanicked => None,
        })
        .try_fold(0u64, |total, value| total.checked_add(value))
        .expect(
            "INVARIANT VIOLATED: blocking single-flight retained weight overflowed u64. This is a bug because the configured process memory budget is far below u64::MAX. Fix: validate synchronous product weight accounting before retention.",
        )
}

#[cfg(test)]
mod tests {
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
        assert_eq!(stats.lookups, 16);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.producers, 1);
        assert_eq!(stats.hits + stats.joined_flights, 15);
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
        assert_eq!(stats.lookups, 9);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.producers, 2);
        assert_eq!(stats.failures, 1);
    }

    #[tokio::test]
    async fn bounded_cache_evicts_oldest_completed_values_by_weight() {
        let cache =
            BoundedSingleFlightCache::<String, Vec<u8>>::new(3, 10, |value| value.len() as u64);

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
        assert_eq!(cache.snapshot().evictions, 1);
    }

    #[tokio::test]
    async fn overweight_value_serves_current_waiters_without_being_retained() {
        let cache =
            BoundedSingleFlightCache::<String, Vec<u8>>::new(2, 4, |value| value.len() as u64);
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

        while cache.snapshot().joined_flights == 0 {
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
        assert_eq!(cache.snapshot().producers, 1);
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
        while cache.snapshot().joined_flights == 0 {
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
        assert_eq!(cache.snapshot().producers, 1);
        assert_eq!(cache.snapshot().joined_flights, 1);
        assert_eq!(cache.snapshot().hits, 1);
    }

    #[test]
    fn blocking_bounded_cache_retries_failed_products_and_evicts_by_weight() {
        let cache =
            BlockingBoundedSingleFlightCache::<String, Vec<u8>, String>::new(2, 4, |value| {
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
        assert_eq!(snapshot.failures, 1);
        assert_eq!(snapshot.producers, 3);
        assert_eq!(snapshot.evictions, 1);
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
        while cache.snapshot().joined_flights == 0 {
            std::thread::yield_now();
        }
        release_tx.send(()).unwrap();

        assert!(producer.join().is_err());
        assert!(waiter.join().is_err());
        assert!(cache.is_empty());
        assert_eq!(cache.snapshot().failures, 1);
        assert_eq!(
            *cache
                .get_or_try_init("panic".to_string(), || Ok(42))
                .unwrap(),
            42
        );
        assert_eq!(cache.snapshot().producers, 2);
    }
}
