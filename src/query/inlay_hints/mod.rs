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

use crate::query::EngineQuery;
use crate::types::ruby_document::RubyDocument;
use tower_lsp::lsp_types::Range;

impl EngineQuery {
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
            file_id: document.analysis_file_id(),
            analysis_engine: self.analysis_engine().cloned(),
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
