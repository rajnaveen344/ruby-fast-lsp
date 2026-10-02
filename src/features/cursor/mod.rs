//! Cursor context shared by editor features: `EngineQuery` pairs an optional
//! open document with its owning project's analysis engine, and
//! `analysis_location` converts domain ranges to protocol locations.
//!
//! ```no_run
//! use std::sync::Arc;
//! use parking_lot::RwLock;
//! use ruby_analysis::engine::AnalysisEngine;
//! use ruby_fast_lsp::features::cursor::EngineQuery;
//! use tower_lsp::lsp_types::{Position, Url};
//!
//! // Supply the owning project's populated engine when querying real sources.
//! let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
//! let query = EngineQuery::with_engine(engine);
//! let uri = Url::parse("file:///example.rb").unwrap();
//! let definitions = query.find_definitions_at_position(&uri, Position::new(0, 0), "");
//! ```

pub(crate) mod analysis_location;
mod method;

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
        invariant_eq!(
            &document.uri,
            uri,
            what = "EngineQuery document URI differs from the analyzed request URI",
            why = "execution-context facts are file-local",
            fix = "construct EngineQuery with the request's owning document",
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
        .view()
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
