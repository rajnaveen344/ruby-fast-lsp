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

use crate::indexer::entry::EntryKind;
use crate::indexer::index::EntryId;
use crate::inferrer::r#type::ruby::RubyType;
use crate::query::IndexQuery;
use crate::types::ruby_document::RubyDocument;
use crate::utils::ast::find_def_node_at_line;
use tower_lsp::lsp_types::{Range, Url};

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
        let uri = &document.uri;

        // Step 1: Trigger method return type inference for visible methods
        self.infer_and_update_visible_types(uri, content, range);

        // Step 2: Parse AST
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        // Step 3: Collect relevant nodes
        let collector = InlayNodeCollector::new(document, *range, content.as_bytes());
        let nodes = collector.collect(&root);

        // Step 4: Create hint context
        let context = HintContext {
            index: self.index.clone(),
            uri,
            content,
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

    /// Infer return types for methods in the visible range and update the index.
    pub fn infer_and_update_visible_types(&self, uri: &Url, content: &str, range: &Range) {
        // Collect only method entries that:
        // 1. Are within the visible range
        // 2. Need inference (return_type is None)
        let methods_needing_inference: Vec<(u32, EntryId)> = {
            let index = self.index.lock();
            index
                .get_entry_ids_for_uri(uri)
                .iter()
                .filter_map(|&entry_id| {
                    if let Some(entry) = index.get_entry(entry_id) {
                        if let EntryKind::Method(data) = &entry.kind {
                            // Check if method is within visible range
                            let method_line = entry.location.range.start.line;
                            if method_line >= range.start.line && method_line <= range.end.line {
                                // Only include if needs inference
                                if data.return_type.is_none() {
                                    if let Some(pos) = data.return_type_position {
                                        return Some((pos.line, entry_id));
                                    }
                                }
                            }
                        }
                    }
                    None
                })
                .collect()
        };

        // Fast path: nothing to infer
        if methods_needing_inference.is_empty() {
            return;
        }

        // Parse the file ONCE and infer only the visible methods
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        // Create file content map for recursive inference
        let mut file_contents = std::collections::HashMap::new();
        file_contents.insert(uri, content.as_bytes());

        // Infer and cache (no lock held during inference)
        let inferred_types: Vec<(EntryId, RubyType)> = methods_needing_inference
            .iter()
            .filter_map(|(line, entry_id)| {
                let def_node = find_def_node_at_line(&node, *line, content)?;

                // We lock the index briefly here for each method to get context
                let mut index = self.index.lock();

                // Get owner FQN from the entry to provide context for inference
                let owner_fqn = index.get_entry(*entry_id).and_then(|e| {
                    if let EntryKind::Method(m) = &e.kind {
                        Some(m.owner.clone())
                    } else {
                        None
                    }
                });

                // Call inference logic
                let inferred_ty = crate::inferrer::return_type::infer_return_type_for_node(
                    &mut index,
                    content.as_bytes(),
                    &def_node,
                    owner_fqn,
                    Some(&file_contents),
                )?;

                Some((*entry_id, inferred_ty))
            })
            .collect();

        // Update the index with results
        if !inferred_types.is_empty() {
            let mut index = self.index.lock();
            for (entry_id, inferred_ty) in inferred_types {
                index.update_method_return_type(entry_id, inferred_ty);
            }
        }
    }

    /// Helper to resolve local variable type using inference if needed.
    pub fn resolve_local_var_type(
        &self,
        content: &str,
        name: &str,
        known_type: Option<&RubyType>,
        type_narrowing: Option<RubyType>,
    ) -> Option<RubyType> {
        // 1. Try type narrowing
        if let Some(ty) = type_narrowing {
            if ty != RubyType::Unknown {
                return Some(ty);
            }
        }

        // 2. Try known type from assignment tracking
        if let Some(ty) = known_type {
            if *ty != RubyType::Unknown {
                return Some(ty.clone());
            }
        }

        // 3. Try fallback inference
        let index = self.index.lock();
        crate::query::infer_type_from_assignment(content, name, &index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::index_ref::Index;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use tower_lsp::lsp_types::{Position, Url};

    fn create_test_query() -> IndexQuery {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(Mutex::new(index)));
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
