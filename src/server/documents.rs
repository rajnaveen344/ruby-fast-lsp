//! Open editor buffers and per-document lifecycle serialization.
use parking_lot::{Mutex, MutexGuard, RwLock};
use ruby_analysis::core::SourceFileId;
use ruby_analysis::indexer::RubyDocument;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tower_lsp::lsp_types::Url;

type DocumentMap = HashMap<Url, Arc<RwLock<RubyDocument>>>;

#[derive(Clone, Default)]
pub(crate) struct OpenDocuments {
    entries: Arc<Mutex<DocumentMap>>,
    semantic_locks: Arc<Mutex<HashMap<Url, Weak<tokio::sync::Mutex<()>>>>>,
}

/// Read-only view: callers cannot bypass insertion/removal operations.
pub(crate) struct OpenDocumentView<'a> {
    entries: MutexGuard<'a, DocumentMap>,
}
impl OpenDocumentView<'_> {
    pub(crate) fn get(&self, uri: &Url) -> Option<&Arc<RwLock<RubyDocument>>> {
        self.entries.get(uri)
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Url, &Arc<RwLock<RubyDocument>>)> {
        self.entries.iter()
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &Arc<RwLock<RubyDocument>>> {
        self.entries.values()
    }
    pub(crate) fn keys(&self) -> impl Iterator<Item = &Url> {
        self.entries.keys()
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn contains_key(&self, uri: &Url) -> bool {
        self.entries.contains_key(uri)
    }
}
impl OpenDocuments {
    pub(crate) fn read(&self) -> OpenDocumentView<'_> {
        OpenDocumentView {
            entries: self.entries.lock(),
        }
    }
    pub(crate) fn insert(&self, uri: Url, document: Arc<RwLock<RubyDocument>>) {
        self.entries.lock().insert(uri, document);
    }
    pub(crate) fn remove(&self, uri: &Url) {
        self.entries.lock().remove(uri);
    }
    pub(crate) fn update(&self, uri: &Url, file_id: SourceFileId, content: String, version: i32) {
        let mut entries = self.entries.lock();
        if let Some(document) = entries.get(uri) {
            let mut document = document.write();
            document.set_analysis_file_id(file_id);
            document.update(content, version);
        } else {
            let document =
                RubyDocument::with_analysis_file_id(uri.clone(), content, version, file_id);
            entries.insert(uri.clone(), Arc::new(RwLock::new(document)));
        }
    }
    pub(crate) fn semantic_lock(&self, uri: &Url) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.semantic_locks.lock();
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(uri).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(uri.clone(), Arc::downgrade(&lock));
        lock
    }
}
