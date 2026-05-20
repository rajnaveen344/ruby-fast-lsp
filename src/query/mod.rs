//! Query Engine - Unified query layer for analysis facts
//!
//! This module provides a single entry point for all index queries, consolidating
//! business logic that was previously scattered across capabilities.
//!
//! # Architecture
//!
//! ```text
//! server.rs (API) → query/ (Service) → analysis-engine/ (Data)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! let query = EngineQuery::with_engine(server.analysis_engine.clone());
//! let definitions = query.find_definitions(&uri, position, &content, None);
//! ```

pub(crate) mod analysis_location;
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
pub use hover::HoverInfo;
pub use inlay_hints::{InlayHintData, InlayHintKind};
pub use method::{MethodCalleeResolution, MethodInfo, ResolvedMethodCallee};
pub use types::TypeQuery;

use crate::types::ruby_document::RubyDocument;
use parking_lot::{Mutex, RwLock};
use ruby_analysis_engine::AnalysisEngine;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

/// Unified query interface for analysis-backed LSP features.
///
/// Provides high-level, domain-focused query methods that abstract away
/// analysis-engine details.
pub struct EngineQuery {
    doc: Option<Arc<RwLock<RubyDocument>>>,
    uri: Option<Url>,
    analysis_engine: Option<Arc<Mutex<AnalysisEngine>>>,
}

impl EngineQuery {
    /// Create a new EngineQuery for index-wide queries.
    pub fn new<_Index>(_index: _Index) -> Self {
        Self {
            doc: None,
            uri: None,
            analysis_engine: None,
        }
    }

    /// Create an EngineQuery with full document context.
    /// This enables access to local variables and AST-based queries.
    pub fn with_doc<_Index>(_index: _Index, doc: Arc<RwLock<RubyDocument>>) -> Self {
        let uri = doc.read().uri.clone();
        Self {
            doc: Some(doc),
            uri: Some(uri),
            analysis_engine: None,
        }
    }

    /// Create an EngineQuery with document context and analysis engine access.
    pub fn with_doc_and_engine(
        doc: Arc<RwLock<RubyDocument>>,
        analysis_engine: Arc<Mutex<AnalysisEngine>>,
    ) -> Self {
        let uri = doc.read().uri.clone();
        Self {
            doc: Some(doc),
            uri: Some(uri),
            analysis_engine: Some(analysis_engine),
        }
    }

    /// Create an EngineQuery with analysis engine access and no document context.
    pub fn with_engine(analysis_engine: Arc<Mutex<AnalysisEngine>>) -> Self {
        Self {
            doc: None,
            uri: None,
            analysis_engine: Some(analysis_engine),
        }
    }

    /// Create an EngineQuery with just a URI (no document).
    pub fn with_uri<_Index>(_index: _Index, uri: Url) -> Self {
        Self {
            doc: None,
            uri: Some(uri),
            analysis_engine: None,
        }
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

impl Clone for EngineQuery {
    fn clone(&self) -> Self {
        Self {
            doc: self.doc.clone(),
            uri: self.uri.clone(),
            analysis_engine: self.analysis_engine.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_query_creation() {
        let query = EngineQuery::new(());
        assert!(query.uri().is_none());
        assert!(query.doc().is_none());
    }

    #[test]
    fn test_index_query_with_uri() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let query = EngineQuery::with_uri((), uri.clone());
        assert_eq!(query.uri(), Some(&uri));
    }
}
