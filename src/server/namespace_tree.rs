//! Editor projection cache; semantic identity stays with the analysis engine.
use super::RubyLanguageServer;
use crate::query::namespace_tree::NamespaceTreeResponse;
use log::debug;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Clone, Default)]
pub(super) struct NamespaceTreeCache {
    response: Arc<Mutex<Option<(u64, NamespaceTreeResponse)>>>,
    invalidation: Arc<Mutex<Option<Instant>>>,
}
impl NamespaceTreeCache {
    pub(super) fn get(&self, hash: u64) -> Option<NamespaceTreeResponse> {
        self.response
            .lock()
            .as_ref()
            .filter(|(stored, _)| *stored == hash)
            .map(|(_, response)| response.clone())
    }
    pub(super) fn store(&self, hash: u64, response: NamespaceTreeResponse) {
        *self.response.lock() = Some((hash, response));
    }
}
impl RubyLanguageServer {
    pub(crate) fn cached_namespace_tree(&self, hash: u64) -> Option<NamespaceTreeResponse> {
        self.namespace_tree.get(hash)
    }
    pub(crate) fn cache_namespace_tree(&self, hash: u64, response: NamespaceTreeResponse) {
        self.namespace_tree.store(hash, response);
    }
}

impl NamespaceTreeCache {
    fn invalidate_debounced(&self) {
        let cache = self.clone();
        tokio::spawn(async move {
            // Set the timer to current time
            {
                let mut timer = cache.invalidation.lock();
                *timer = Some(Instant::now());
            }

            // Wait for the debounce period
            sleep(Duration::from_millis(300)).await;

            // Check if we should still invalidate (no newer timer was set)
            let should_invalidate = {
                let timer = cache.invalidation.lock();
                if let Some(timer_instant) = *timer {
                    timer_instant.elapsed() >= Duration::from_millis(300)
                } else {
                    false
                }
            };

            if should_invalidate {
                *cache.response.lock() = None;
                debug!("Namespace tree cache invalidated after debounce period");

                // Clear the timer
                *cache.invalidation.lock() = None;
            }
        });
    }
}
impl RubyLanguageServer {
    pub fn invalidate_namespace_tree_cache_debounced(&self) {
        self.namespace_tree.invalidate_debounced();
    }
}
