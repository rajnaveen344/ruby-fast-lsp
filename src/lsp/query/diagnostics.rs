//! Diagnostics Query — analysis-engine diagnostic projection.
//!
//! Provides diagnostics for:
//! - Unresolved constants and methods
//!
//! The projection itself is server-owned (`server::diagnostics`) so that
//! publication and queries share one conversion. AST-only diagnostics (syntax
//! errors/warnings) live in `loader/file_processor/syntax_diagnostics.rs`.

use crate::invariant::ExpectInvariant;
use crate::server::unresolved_diagnostics_from_engine;
use tower_lsp::lsp_types::{Diagnostic, Url};

use super::EngineQuery;

impl EngineQuery {
    /// Get diagnostics for unresolved entries from the analysis engine.
    pub fn get_unresolved_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let analysis_engine = self.analysis_engine.as_ref().expect_invariant(
            "unresolved diagnostics requested without analysis engine",
            "diagnostics are owned by ruby-analysis::engine",
            "construct EngineQuery with EngineQuery::with_engine or with_doc_and_engine",
        );
        let engine = analysis_engine.read();
        unresolved_diagnostics_from_engine(&engine, uri)
    }
}
