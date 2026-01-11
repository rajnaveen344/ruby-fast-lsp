//! Query Engine - Unified query layer for RubyIndex
//!
//! This module provides a single entry point for all index queries, consolidating
//! business logic that was previously scattered across capabilities.
//!
//! # Architecture
//!
//! ```text
//! server.rs (API) → query/ (Service) → indexer/ (Data)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! let query = IndexQuery::new(server.index.clone());
//! let definitions = query.find_definitions(&uri, position, &content, None);
//! ```

mod definition;
mod hover;
mod inlay_hints;
mod method;
mod references;
mod types;

pub use hover::HoverInfo;
pub use inlay_hints::InlayHintData;
pub use method::MethodInfo;
pub use types::{infer_type_from_assignment, TypeHint, TypeHintKind, TypeQuery};

use crate::indexer::index_ref::{Index, Unlocked};
use crate::types::ruby_document::RubyDocument;
use parking_lot::RwLock;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

/// Unified query interface for RubyIndex.
///
/// Provides high-level, domain-focused query methods that abstract away
/// the low-level index details. All index-heavy capability logic should
/// use this interface.
///
/// Uses `Index<Unlocked>` internally so methods can lock/unlock as needed,
/// avoiding deadlocks when calling other methods that need index access.
pub struct IndexQuery {
    index: Index<Unlocked>,
    doc: Option<Arc<RwLock<RubyDocument>>>,
    uri: Option<Url>,
}

impl IndexQuery {
    /// Create a new IndexQuery for index-wide queries.
    pub fn new(index: Index<Unlocked>) -> Self {
        Self {
            index,
            doc: None,
            uri: None,
        }
    }

    /// Create an IndexQuery with full document context.
    /// This enables access to local variables and AST-based queries.
    pub fn with_doc(index: Index<Unlocked>, doc: Arc<RwLock<RubyDocument>>) -> Self {
        let uri = doc.read().uri.clone();
        Self {
            index,
            doc: Some(doc),
            uri: Some(uri),
        }
    }

    /// Create an IndexQuery with just a URI (no document).
    pub fn with_uri(index: Index<Unlocked>, uri: Url) -> Self {
        Self {
            index,
            doc: None,
            uri: Some(uri),
        }
    }

    /// Get a clone of the index handle.
    #[inline]
    pub fn index_ref(&self) -> Index<Unlocked> {
        self.index.clone()
    }

    /// Get the current file URI if set.
    #[inline]
    pub fn uri(&self) -> Option<&Url> {
        self.uri.as_ref()
    }

    /// Get the document if attached.
    #[inline]
    pub fn doc(&self) -> Option<&Arc<RwLock<RubyDocument>>> {
        self.doc.as_ref()
    }
}

impl Clone for IndexQuery {
    fn clone(&self) -> Self {
        Self {
            index: self.index.clone(),
            doc: self.doc.clone(),
            uri: self.uri.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use parking_lot::Mutex;

    #[test]
    fn test_index_query_creation() {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(Mutex::new(index)));
        let query = IndexQuery::new(index_ref);
        assert!(query.uri().is_none());
        assert!(query.doc().is_none());
    }

    #[test]
    fn test_index_query_with_uri() {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(Mutex::new(index)));
        let uri = Url::parse("file:///test.rb").unwrap();
        let query = IndexQuery::with_uri(index_ref, uri.clone());
        assert_eq!(query.uri(), Some(&uri));
    }
}
