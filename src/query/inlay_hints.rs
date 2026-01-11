use crate::indexer::entry::EntryKind;
use crate::indexer::index::EntryId;
use crate::inferrer::r#type::ruby::RubyType;
use crate::query::{infer_type_from_assignment, IndexQuery};
use tower_lsp::lsp_types::{Position, Range, Url};

/// Data structure for inlay hints
#[derive(Debug, Clone)]
pub struct InlayHintData {
    pub position: Position,
    pub label: String,
    pub kind: InlayHintKind,
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InlayHintKind {
    Type,
    Parameter,
}

impl IndexQuery {
    /// Infer return types for methods in the visible range and update the index.
    /// This logic was moved from capabilities/inlay_hints.rs to encapsulate index access.
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

                // Call inference logic (requires mutable index for caching results internally)
                // Note: infer_return_type_for_node uses &mut index but we have MutexGuard
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

        // Update the index (brief lock) with results
        if !inferred_types.is_empty() {
            let mut index = self.index.lock();
            for (entry_id, inferred_ty) in inferred_types {
                index.update_method_return_type(entry_id, inferred_ty);
            }
        }
    }

    /// Helper to get local variable type using inference if needed
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
        infer_type_from_assignment(content, name, &index)
    }
}

/// Helper to find DefNode at line (copied from inlay_hints.rs context)
fn find_def_node_at_line<'a>(
    node: &ruby_prism::Node<'a>,
    target_line: u32,
    content: &str,
) -> Option<ruby_prism::DefNode<'a>> {
    // Try to match DefNode
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        // Calculate line from byte offset (count newlines before this offset)
        let line = content.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes (Program, Class, Module, Statements)
    // Simplified traversal for brevity - ensuring we cover main structures
    if let Some(program) = node.as_program_node() {
        return find_in_statements(&program.statements(), target_line, content);
    }
    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                return find_in_statements(&stmts, target_line, content);
            }
        }
    }
    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                return find_in_statements(&stmts, target_line, content);
            }
        }
    }
    if let Some(stmts) = node.as_statements_node() {
        return find_in_statements(&stmts, target_line, content);
    }

    None
}

fn find_in_statements<'a>(
    stmts: &ruby_prism::StatementsNode<'a>,
    target_line: u32,
    content: &str,
) -> Option<ruby_prism::DefNode<'a>> {
    for stmt in stmts.body().iter() {
        if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
            return Some(found);
        }
    }
    None
}
