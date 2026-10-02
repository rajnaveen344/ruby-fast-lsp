//! Diagnostics: the engine's unresolved-entry diagnostics for a document, and
//! the external linter and formatter runner.
//!
//! The engine-to-LSP projection itself is server-owned (`server::diagnostics`)
//! so that publication and queries share one conversion. AST-only diagnostics
//! (syntax errors and warnings) live in
//! `loader/file_processor/syntax_diagnostics.rs`.

pub mod linter;

use crate::invariant::ExpectInvariant;
use crate::server::unresolved_diagnostics_from_engine;
use tower_lsp::lsp_types::{Diagnostic, Url};

use crate::features::cursor::EngineQuery;

impl EngineQuery {
    /// Get diagnostics for unresolved entries from the analysis engine.
    pub fn get_unresolved_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let analysis_engine = self.analysis_engine().expect_invariant(
            "unresolved diagnostics requested without analysis engine",
            "diagnostics are owned by ruby-analysis::engine",
            "construct EngineQuery with EngineQuery::with_engine or with_doc_and_engine",
        );
        let engine = analysis_engine.read();
        unresolved_diagnostics_from_engine(&engine, uri)
    }
}
