use super::RubyLanguageServer;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;
use tower_lsp::lsp_types::FileEvent;

#[derive(Debug, Default)]
pub(super) struct WatchedFileChangeBatch {
    generation: u64,
    changes: BTreeMap<String, FileEvent>,
}

impl WatchedFileChangeBatch {
    pub(super) fn queue(&mut self, changes: Vec<FileEvent>) -> u64 {
        self.generation = self.generation.checked_add(1).expect(
            "INVARIANT VIOLATED: watched-file debounce generation overflowed. This is a bug because one server cannot receive 2^64 watched-file batches. Fix: inspect the client watcher storm that exhausted the generation counter.",
        );
        for change in changes {
            self.changes.insert(change.uri.to_string(), change);
        }
        self.generation
    }

    pub(super) fn take(&mut self, generation: u64) -> Option<Vec<FileEvent>> {
        if generation != self.generation || self.changes.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.changes).into_values().collect())
    }

    pub(super) fn cancel(&mut self) {
        self.generation = self.generation.checked_add(1).expect(
            "INVARIANT VIOLATED: watched-file debounce generation overflowed during cancellation. This is a bug because one server cannot cancel 2^64 watcher batches. Fix: inspect the shutdown or workspace lifecycle loop that exhausted the generation counter.",
        );
        self.changes.clear();
    }
}

#[derive(Clone, Default)]
pub(super) struct WatchedFileChanges {
    batch: Arc<Mutex<WatchedFileChangeBatch>>,
}

impl RubyLanguageServer {
    pub(crate) fn queue_watched_file_changes(&self, changes: Vec<FileEvent>) -> u64 {
        self.file_changes.queue(changes)
    }

    pub(crate) fn take_watched_file_changes(&self, generation: u64) -> Option<Vec<FileEvent>> {
        self.file_changes.take(generation)
    }

    pub(crate) fn cancel_watched_file_changes(&self) {
        self.file_changes.cancel();
    }
}

impl WatchedFileChanges {
    fn queue(&self, changes: Vec<FileEvent>) -> u64 {
        self.batch.lock().queue(changes)
    }
    fn take(&self, generation: u64) -> Option<Vec<FileEvent>> {
        self.batch.lock().take(generation)
    }
    fn cancel(&self) {
        self.batch.lock().cancel();
    }
}
