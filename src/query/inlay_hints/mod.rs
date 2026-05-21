//! Inlay Hints Query Module
//!
//! This module provides a clean, principled implementation of inlay hints:
//!
//! 1. `ruby-analysis::indexer::inlay_hints`: AST collection into domain nodes
//! 2. **Generators** (`generators.rs`): Convert nodes to LSP hint data
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
//!      ├─► ruby_analysis::indexer::inlay_hints::InlayNodeCollector.collect()
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

mod generators;

pub use generators::{
    generate_chained_call_hints, generate_method_hints, generate_structural_hints,
    generate_variable_type_hints, HintContext, InlayHintData, InlayHintKind,
};

use crate::query::EngineQuery;
use ruby_analysis::indexer::{inlay_hints::InlayNodeCollector, RubyDocument};
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

        let range = document.lsp_range_to_text_range(*range);

        // Step 3: Collect relevant domain nodes
        let collector =
            InlayNodeCollector::new(range.start_byte, range.end_byte, content.as_bytes());
        let nodes = collector.collect(&root);

        // Step 4: Create hint context
        let context = HintContext {
            file_id: document.analysis_file_id(),
            document,
            analysis_engine: self.analysis_engine().cloned(),
        };

        // Step 5: Generate hints from collected nodes
        let mut hints = Vec::new();

        // Structural hints (end labels, implicit returns)
        hints.extend(generate_structural_hints(&nodes, &context));

        // Variable type hints
        hints.extend(generate_variable_type_hints(&nodes, &context));

        // Method return type and parameter hints
        hints.extend(generate_method_hints(&nodes, &context));

        // Chained method call hints (currently placeholder)
        hints.extend(generate_chained_call_hints(&nodes, &context));

        hints
    }
}
