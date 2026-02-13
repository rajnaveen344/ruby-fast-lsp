//! Completion Query — Provides index-backed completion lookups.
//!
//! Wraps constant and method completion logic behind `IndexQuery`,
//! keeping lock management in one place.

use tower_lsp::lsp_types::{CompletionItem, Position};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::capabilities::completion::constant;
use crate::capabilities::completion::method;
use crate::indexer::entry::NamespaceKind;
use crate::inferrer::r#type::ruby::RubyType;

use super::IndexQuery;

impl IndexQuery {
    /// Find constant completions by locking the index and delegating
    /// to the existing constant completion engine.
    pub fn find_constant_completions(
        &self,
        analyzer: &RubyPrismAnalyzer,
        position: Position,
        partial: String,
    ) -> Vec<CompletionItem> {
        let index = self.index.lock();
        constant::find_constant_completions(&index, analyzer, position, partial)
    }

    /// Find method completions for a receiver type.
    ///
    /// Delegates to `method::find_method_completions` which handles
    /// both RBS (built-in) and index (user-defined) methods.
    pub fn find_method_completions(
        &self,
        receiver_type: &RubyType,
        partial_method: &str,
        kind: NamespaceKind,
    ) -> Vec<CompletionItem> {
        method::find_method_completions(&self.index, receiver_type, partial_method, kind)
    }
}
