//! Cursor context shared by editor features: `EngineQuery` pairs an optional
//! open document with its owning project's analysis engine, `Cursor` is one
//! request's consistent read of both, and `analysis_location` converts domain
//! ranges to protocol locations.
//!
//! ```no_run
//! use std::sync::Arc;
//! use parking_lot::RwLock;
//! use ruby_analysis::engine::AnalysisEngine;
//! use ruby_fast_lsp::features::cursor::EngineQuery;
//!
//! // Supply the owning project's populated engine when querying real sources.
//! let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
//! let query = EngineQuery::with_engine(engine);
//! let files = query.with_view(|cursor| cursor.view.files().count());
//! ```

pub(crate) mod analysis_location;
pub(crate) mod method;

use crate::utils::lsp::source_position;
use parking_lot::RwLock;
use ruby_analysis::core::SourceFileId;
use ruby_analysis::engine::{AnalysisEngine, View};
use ruby_analysis::indexer::{RubyDocument, RubyPrismAnalyzer};
use std::sync::Arc;
use tower_lsp::lsp_types::{Position, Url};

/// Protocol-facing query interface for analysis-backed LSP features.
///
/// Keeps `tower_lsp` response construction in `ruby-fast-lsp` while semantic
/// lookup stays in `ruby-analysis`.
#[derive(Clone)]
pub struct EngineQuery {
    doc: Option<Arc<RwLock<RubyDocument>>>,
    uri: Option<Url>,
    analysis_engine: Arc<RwLock<AnalysisEngine>>,
}

/// One request's read state: the open document, when the request has one, and
/// one view of its owning engine. [`EngineQuery::with_view`] takes each read
/// guard once for the whole request; functions over a cursor never lock.
#[derive(Clone, Copy)]
pub struct Cursor<'a> {
    pub view: &'a View<'a>,
    pub document: Option<&'a RubyDocument>,
}

impl Cursor<'_> {
    /// The open document's analysis file identity.
    pub fn file_id(&self) -> Option<SourceFileId> {
        self.document.map(RubyDocument::analysis_file_id)
    }

    /// The open document's analysis byte offset for a protocol position.
    pub fn offset(&self, position: Position) -> Option<u32> {
        self.document
            .map(|document| document.position_to_analysis_offset(source_position(position)))
    }

    /// A cursor analyzer over `content`, with the document's execution context
    /// at `position` when the request has an open document.
    pub(crate) fn analyzer(
        &self,
        uri: &Url,
        content: &str,
        position: Position,
    ) -> RubyPrismAnalyzer {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let Some(document) = self.document else {
            return analyzer;
        };
        invariant_eq!(
            &document.uri,
            uri,
            what = "EngineQuery document URI differs from the analyzed request URI",
            why = "execution-context facts are file-local",
            fix = "construct EngineQuery with the request's owning document",
        );
        analyzer_for_document(analyzer, document, self.view, position)
    }
}

impl EngineQuery {
    /// Run `read` over one consistent cursor: the document read guard, then
    /// the engine read guard, each taken exactly once and released on return.
    /// `read` is synchronous, so no guard is held across an `.await`.
    pub fn with_view<R>(&self, read: impl FnOnce(Cursor<'_>) -> R) -> R {
        let document = self.doc.as_ref().map(|document| document.read());
        let engine = self.analysis_engine.read();
        let view = engine.view();
        read(Cursor {
            view: &view,
            document: document.as_deref(),
        })
    }

    pub(crate) fn analyzer_at_position(
        &self,
        uri: &Url,
        content: &str,
        position: Position,
    ) -> RubyPrismAnalyzer {
        self.with_view(|cursor| cursor.analyzer(uri, content, position))
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
            analysis_engine,
        }
    }

    /// Create an EngineQuery with analysis engine access and no document context.
    pub fn with_engine(analysis_engine: Arc<RwLock<AnalysisEngine>>) -> Self {
        Self {
            doc: None,
            uri: None,
            analysis_engine,
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

    /// Get the analysis engine.
    #[inline]
    pub fn analysis_engine(&self) -> Option<&Arc<RwLock<AnalysisEngine>>> {
        Some(&self.analysis_engine)
    }
}

pub(crate) fn analyzer_for_document(
    analyzer: RubyPrismAnalyzer,
    document: &RubyDocument,
    view: &View<'_>,
    position: Position,
) -> RubyPrismAnalyzer {
    let byte_offset = document.position_to_analysis_offset(source_position(position));
    match view.execution_context_at(document.analysis_file_id(), byte_offset) {
        Some(context) => analyzer.with_execution_context(context.clone()),
        None => analyzer,
    }
}
