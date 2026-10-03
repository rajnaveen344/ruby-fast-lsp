//! Open editor buffers and per-document lifecycle serialization.
use super::Server;
use crate::loader::context::{OpenDocumentVersion, SourceReader};
use crate::loader::file_processor::FileProcessor;
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

/// Live loader view: each call observes the open buffers at that moment.
impl SourceReader for OpenDocuments {
    fn open_uris(&self) -> Vec<Url> {
        self.read().keys().cloned().collect()
    }
    fn open_document(&self, uri: &Url) -> Option<RubyDocument> {
        self.read().get(uri).map(|document| document.read().clone())
    }
    fn open_document_version(&self, uri: &Url) -> Option<OpenDocumentVersion> {
        self.read().get(uri).map(|document| {
            let document = document.read();
            OpenDocumentVersion {
                version: document.version,
                indexed_version: document.indexed_version,
            }
        })
    }
    fn visit_open_documents(&self, visit: &mut dyn FnMut(&RubyDocument)) {
        let mut documents = self.read().values().cloned().collect::<Vec<_>>();
        documents.sort_by(|left, right| left.read().uri.cmp(&right.read().uri));
        for document in documents {
            visit(&document.read());
        }
    }
}

impl Server {
    /// Store an embedded server's open buffer and run its current-file
    /// analysis pass without editor notifications or diagnostics publication.
    /// Measurement tools use this to observe the open-document lifecycle alone.
    pub fn open_embedded_document(
        &self,
        uri: &Url,
        content: &str,
        version: i32,
    ) -> anyhow::Result<()> {
        {
            let mut entries = self.documents.entries.lock();
            if let Some(document) = entries.get(uri) {
                document.write().update(content.to_string(), version);
            } else {
                let document = RubyDocument::new(uri.clone(), content.to_string(), version);
                entries.insert(uri.clone(), Arc::new(RwLock::new(document)));
            }
        }
        let ctx = self.load_context_for_uri(uri);
        let loaded = FileProcessor::with_extension_registry(self.extensions.registry().clone())
            .analyze_file_current_file_resolution(uri, content, &ctx)?;
        loaded.commit(&ctx);
        Ok(())
    }

    /// Drop an embedded server's open buffer. Analysis facts stay, matching
    /// editor close semantics for cross-file navigation.
    pub fn close_embedded_document(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// The open buffer for `uri`, shared so callers can release the map lock.
    pub(crate) fn open_document(&self, uri: &Url) -> Option<Arc<RwLock<RubyDocument>>> {
        self.documents.read().get(uri).cloned()
    }

    /// A copy of the open buffer's current text.
    pub(crate) fn open_document_content(&self, uri: &Url) -> Option<String> {
        let document = self.open_document(uri)?;
        let content = document.read().content.clone();
        Some(content)
    }

    pub(crate) fn is_document_open(&self, uri: &Url) -> bool {
        self.documents.read().contains_key(uri)
    }

    /// Read-only view of every open buffer, held only while iterating.
    pub(crate) fn open_documents(&self) -> OpenDocumentView<'_> {
        self.documents.read()
    }

    /// Apply the editor's latest text to an open buffer, opening it if new.
    pub(crate) fn update_open_document(
        &self,
        uri: &Url,
        file_id: SourceFileId,
        content: String,
        version: i32,
    ) {
        self.documents.update(uri, file_id, content, version);
    }

    /// Forget the editor's buffer. Analysis facts stay for cross-file navigation.
    pub(crate) fn close_open_document(&self, uri: &Url) {
        self.documents.remove(uri);
    }
}
