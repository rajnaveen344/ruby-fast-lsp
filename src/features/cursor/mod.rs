//! Cursor context shared by editor features: `EngineQuery` pairs an optional
//! open document with its owning project's analysis engine, `Cursor` is one
//! request's consistent read of both, and `analysis_location` converts domain
//! ranges to protocol locations.
//!
//! ```no_run
//! use ruby_fast_lsp::features::cursor::EngineQuery;
//! use ruby_fast_lsp::server::RubyLanguageServer;
//! use tower_lsp::lsp_types::Url;
//!
//! fn file_count(server: &RubyLanguageServer, uri: &Url) -> usize {
//!     let query = EngineQuery::with_project(server.project_for_uri(uri));
//!     query.with_view(|cursor| cursor.view.files().count())
//! }
//! ```

pub(crate) mod analysis_location;
pub(crate) mod method;

use crate::server::ProjectHandle;
use crate::utils::lsp::source_position;
use parking_lot::RwLock;
use ruby_analysis::core::SourceFileId;
use ruby_analysis::engine::View;
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
    project: ProjectHandle,
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
    /// one [`ProjectHandle::view`], each taken exactly once and released on
    /// return. `read` is synchronous, so no guard is held across an `.await`.
    pub fn with_view<R>(&self, read: impl FnOnce(Cursor<'_>) -> R) -> R {
        let document = self.doc.as_ref().map(|document| document.read());
        self.project.view(|view| {
            read(Cursor {
                view,
                document: document.as_deref(),
            })
        })
    }

    /// A query over an open document and the project that owns it.
    pub fn with_doc_and_project(doc: Arc<RwLock<RubyDocument>>, project: ProjectHandle) -> Self {
        Self {
            doc: Some(doc),
            project,
        }
    }

    /// A query over a project with no document context.
    pub fn with_project(project: ProjectHandle) -> Self {
        Self { doc: None, project }
    }

    /// The project this query reads.
    pub fn project(&self) -> &ProjectHandle {
        &self.project
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
