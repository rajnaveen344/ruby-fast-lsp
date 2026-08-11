//! LSP query adapters over analysis facts.
//!
//! This module exposes protocol-facing helpers that keep editor request handling
//! thin while delegating reusable Ruby semantics to `ruby-analysis`.
//!
//! # Architecture
//!
//! ```text
//! server.rs → query/ protocol adapters → ruby-analysis engine/indexer/inference
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
pub(crate) mod definition;
pub mod diagnostics;
mod hover;
mod implementation;
mod inlay_hints;
mod method;
pub mod namespace_tree;
mod references;
mod signature_help;
pub mod type_hierarchy;
mod workspace_symbols;

pub use code_lens::CodeLensData;
pub use hover::HoverInfo;
pub use inlay_hints::{InlayHintData, InlayHintKind};
pub use method::{MethodCalleeResolution, MethodInfo, ResolvedMethodCallee};
pub use ruby_analysis::inference::TypeQuery;
pub use signature_help::{SignatureData, SignatureHelpData, SignatureParameterData};

use crate::utils::lsp::source_position;
use parking_lot::RwLock;
use ruby_analysis::engine::AnalysisEngine;
use ruby_analysis::indexer::{RubyDocument, RubyPrismAnalyzer};
use std::sync::Arc;
use tower_lsp::lsp_types::{Position, Url};

/// Protocol-facing query interface for analysis-backed LSP features.
///
/// Keeps `tower_lsp` response construction in `ruby-fast-lsp` while semantic
/// lookup stays in `ruby-analysis`.
pub struct EngineQuery {
    doc: Option<Arc<RwLock<RubyDocument>>>,
    uri: Option<Url>,
    analysis_engine: Option<Arc<RwLock<AnalysisEngine>>>,
}

impl EngineQuery {
    pub(crate) fn analyzer_at_position(
        &self,
        uri: &Url,
        content: &str,
        position: Position,
    ) -> RubyPrismAnalyzer {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (Some(document), Some(engine)) = (&self.doc, &self.analysis_engine) else {
            return analyzer;
        };
        let document = document.read();
        assert_eq!(
            &document.uri, uri,
            "INVARIANT VIOLATED: EngineQuery document URI differs from the analyzed request URI. This is a bug because execution-context facts are file-local. Fix: construct EngineQuery with the request's owning document."
        );
        analyzer_for_document(analyzer, &document, engine, position)
    }

    /// Create an EngineQuery with document context and analysis engine access.
    pub fn with_doc_and_engine(
        doc: Arc<RwLock<RubyDocument>>,
        analysis_engine: Arc<RwLock<AnalysisEngine>>,
    ) -> Self {
        let uri = doc.read().uri.clone();
        Self {
            doc: Some(doc),
            uri: Some(uri),
            analysis_engine: Some(analysis_engine),
        }
    }

    /// Create an EngineQuery with analysis engine access and no document context.
    pub fn with_engine(analysis_engine: Arc<RwLock<AnalysisEngine>>) -> Self {
        Self {
            doc: None,
            uri: None,
            analysis_engine: Some(analysis_engine),
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
    pub fn analysis_engine(&self) -> Option<&Arc<RwLock<AnalysisEngine>>> {
        self.analysis_engine.as_ref()
    }
}

pub(crate) fn analyzer_for_document(
    analyzer: RubyPrismAnalyzer,
    document: &RubyDocument,
    engine: &Arc<RwLock<AnalysisEngine>>,
    position: Position,
) -> RubyPrismAnalyzer {
    let byte_offset = document.position_to_analysis_offset(source_position(position));
    let context = engine
        .read()
        .query()
        .execution_context_at(document.analysis_file_id(), byte_offset)
        .cloned();
    match context {
        Some(context) => analyzer.with_execution_context(context),
        None => analyzer,
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
