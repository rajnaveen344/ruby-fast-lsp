//! Inlay Hints Query Module
//!
//! This module provides a clean, principled implementation of inlay hints:
//!
//! 1. **Collector** (`collector.rs`): Visits the AST and collects relevant nodes
//! 2. **Nodes** (`nodes.rs`): Data types representing collected AST nodes
//! 3. **Generators** (`generators.rs`): Convert nodes to hints
//!
//! # Architecture
//!
//! ```text
//! LSP Request
//!      │
//!      ▼
//! InlayHintQuery::get_inlay_hints()
//!      │
//!      ├─► Parse AST
//!      │
//!      ├─► InlayNodeCollector.collect()
//!      │        │
//!      │        └─► Vec<InlayNode>
//!      │
//!      ├─► generate_structural_hints()
//!      ├─► generate_variable_type_hints()
//!      ├─► generate_method_hints()
//!      └─► generate_chained_call_hints()
//!               │
//!               └─► Vec<InlayHintData>
//! ```

mod collector;
mod generators;
pub mod nodes;

pub use collector::InlayNodeCollector;
pub use generators::{
    generate_chained_call_hints, generate_method_hints, generate_structural_hints,
    generate_variable_type_hints, HintContext, InlayHintData, InlayHintKind,
};

use crate::query::IndexQuery;
use crate::types::ruby_document::RubyDocument;
use tower_lsp::lsp_types::Range;

impl IndexQuery {
    /// Get all inlay hints for a document within the specified range.
    ///
    /// This is the main entry point for inlay hints. It:
    /// 1. Triggers method return type inference for visible methods
    /// 2. Collects relevant AST nodes via InlayNodeCollector
    /// 3. Generates hints from the collected nodes
    pub fn get_inlay_hints(
        &self,
        document: &RubyDocument,
        range: &Range,
        content: &str,
    ) -> Vec<InlayHintData> {
        // Step 2: Parse AST
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        // Step 3: Collect relevant nodes
        let collector = InlayNodeCollector::new(document, *range, content.as_bytes());
        let nodes = collector.collect(&root);

        // Step 4: Create hint context
        let context = HintContext {
            content,
            type_facts: self
                .analysis_engine()
                .map(|engine| {
                    engine
                        .lock()
                        .type_store()
                        .facts_in_file(document.analysis_file_id())
                })
                .unwrap_or_default(),
            method_facts: self
                .analysis_engine()
                .map(|engine| {
                    engine
                        .lock()
                        .method_store()
                        .facts_in_file(document.analysis_file_id())
                })
                .unwrap_or_default(),
        };

        // Step 5: Generate hints from collected nodes
        let mut hints = Vec::new();

        // Structural hints (end labels, implicit returns)
        hints.extend(generate_structural_hints(&nodes));

        // Variable type hints
        hints.extend(generate_variable_type_hints(&nodes, &context, document));

        // Method return type and parameter hints
        hints.extend(generate_method_hints(&nodes, &context));

        // Chained method call hints (currently placeholder)
        hints.extend(generate_chained_call_hints(&nodes, &context));

        hints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::index_ref::Index;
    use parking_lot::RwLock;
    use std::sync::Arc;
    use tower_lsp::lsp_types::{Position, Url};

    fn create_test_query() -> IndexQuery {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(RwLock::new(index)));
        IndexQuery::new(index_ref)
    }

    #[test]
    fn test_get_inlay_hints_basic() {
        let query = create_test_query();
        let content = "class Foo\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let doc = RubyDocument::new(uri, content.to_string(), 1);
        let range = Range {
            start: Position::new(0, 0),
            end: Position::new(10, 0),
        };

        let hints = query.get_inlay_hints(&doc, &range, content);

        // Should have at least the "class Foo" end hint
        assert!(!hints.is_empty());
        assert!(hints.iter().any(|h| h.label.contains("class Foo")));
    }

    #[test]
    fn test_get_inlay_hints_method() {
        let query = create_test_query();
        let content = "def foo\n  42\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let doc = RubyDocument::new(uri, content.to_string(), 1);
        let range = Range {
            start: Position::new(0, 0),
            end: Position::new(10, 0),
        };

        let hints = query.get_inlay_hints(&doc, &range, content);

        // Should have "def foo" end hint and implicit return
        assert!(hints.iter().any(|h| h.label.contains("def foo")));
        assert!(hints.iter().any(|h| h.label == "return"));
    }
}
