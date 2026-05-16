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
//! let query = IndexQuery::new(server.index_for_uri(&uri));
//! let definitions = query.find_definitions(&uri, position, &content, None);
//! ```

mod analysis_location;
pub mod call_hierarchy;
mod code_lens;
mod completion;
mod debug;
mod definition;
pub mod diagnostics;
mod hover;
mod implementation;
mod inlay_hints;
mod method;
pub mod namespace_tree;
mod references;
pub mod type_hierarchy;
mod types;
mod workspace_symbols;

pub use code_lens::CodeLensData;
pub use diagnostics::generate_yard_diagnostics_inner;
pub use hover::HoverInfo;
pub use inlay_hints::{InlayHintData, InlayHintKind};
pub use method::{MethodCalleeResolution, MethodInfo, ResolvedMethodCallee};
pub use types::{infer_type_from_assignment, TypeQuery};

use crate::indexer::index_ref::{Index, Unlocked};
use crate::types::ruby_document::RubyDocument;
use parking_lot::{Mutex, RwLock};
use ruby_analysis_engine::AnalysisEngine;
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
    analysis_engine: Option<Arc<Mutex<AnalysisEngine>>>,
}

impl IndexQuery {
    /// Create a new IndexQuery for index-wide queries.
    pub fn new(index: Index<Unlocked>) -> Self {
        Self {
            index,
            doc: None,
            uri: None,
            analysis_engine: None,
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
            analysis_engine: None,
        }
    }

    /// Create an IndexQuery with document context and analysis engine access.
    pub fn with_doc_and_engine(
        index: Index<Unlocked>,
        doc: Arc<RwLock<RubyDocument>>,
        analysis_engine: Arc<Mutex<AnalysisEngine>>,
    ) -> Self {
        let uri = doc.read().uri.clone();
        Self {
            index,
            doc: Some(doc),
            uri: Some(uri),
            analysis_engine: Some(analysis_engine),
        }
    }

    /// Create an IndexQuery with analysis engine access and no document context.
    pub fn with_engine(
        index: Index<Unlocked>,
        analysis_engine: Arc<Mutex<AnalysisEngine>>,
    ) -> Self {
        Self {
            index,
            doc: None,
            uri: None,
            analysis_engine: Some(analysis_engine),
        }
    }

    /// Create an IndexQuery with just a URI (no document).
    pub fn with_uri(index: Index<Unlocked>, uri: Url) -> Self {
        Self {
            index,
            doc: None,
            uri: Some(uri),
            analysis_engine: None,
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

    /// Get the analysis engine if attached.
    #[inline]
    pub fn analysis_engine(&self) -> Option<&Arc<Mutex<AnalysisEngine>>> {
        self.analysis_engine.as_ref()
    }
}

impl Clone for IndexQuery {
    fn clone(&self) -> Self {
        Self {
            index: self.index.clone(),
            doc: self.doc.clone(),
            uri: self.uri.clone(),
            analysis_engine: self.analysis_engine.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;

    #[test]
    fn test_index_query_creation() {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(RwLock::new(index)));
        let query = IndexQuery::new(index_ref);
        assert!(query.uri().is_none());
        assert!(query.doc().is_none());
    }

    #[test]
    fn test_index_query_with_uri() {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(RwLock::new(index)));
        let uri = Url::parse("file:///test.rb").unwrap();
        let query = IndexQuery::with_uri(index_ref, uri.clone());
        assert_eq!(query.uri(), Some(&uri));
    }
}
