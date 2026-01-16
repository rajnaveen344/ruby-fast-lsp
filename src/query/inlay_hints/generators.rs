//! Hint generators - Convert collected InlayNodes to InlayHintData.
//!
//! This module contains the logic for generating actual hints from AST nodes.
//! Generators are pure functions that take nodes and context, returning hints.

use super::nodes::{InlayNode, VariableKind};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::ruby_document::RubyDocument;
use tower_lsp::lsp_types::{Position, Url};

/// Unified inlay hint data structure.
#[derive(Debug, Clone)]
pub struct InlayHintData {
    pub position: Position,
    pub label: String,
    pub kind: InlayHintKind,
    pub tooltip: Option<String>,
    pub padding_left: bool,
    pub padding_right: bool,
}

/// The kind of inlay hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlayHintKind {
    // Structural hints
    EndLabel,
    ImplicitReturn,
    // Type hints
    VariableType,
    MethodReturn,
    ParameterType,
    ChainedMethodType,
}

/// Context for hint generation (provides access to type inference).
pub struct HintContext<'a> {
    pub index: Index<Unlocked>,
    pub uri: &'a Url,
    pub content: &'a str,
}

/// Generate structural hints (end labels, implicit returns).
///
/// These hints don't require type inference.
pub fn generate_structural_hints(nodes: &[InlayNode]) -> Vec<InlayHintData> {
    nodes
        .iter()
        .filter_map(|node| match node {
            InlayNode::BlockEnd {
                kind,
                name,
                end_position,
            } => Some(InlayHintData {
                position: *end_position,
                label: format!("{} {}", kind.keyword(), name),
                kind: InlayHintKind::EndLabel,
                tooltip: None,
                padding_left: true,
                padding_right: false,
            }),
            InlayNode::ImplicitReturn { position } => Some(InlayHintData {
                position: *position,
                label: "return".to_string(),
                kind: InlayHintKind::ImplicitReturn,
                tooltip: None,
                padding_left: false,
                padding_right: true,
            }),
            _ => None,
        })
        .collect()
}

/// Generate type hints for variables.
///
/// Uses the index and type narrowing to infer types.
/// Skips constants as they don't typically need type hints.
pub fn generate_variable_type_hints(
    nodes: &[InlayNode],
    context: &HintContext,
    document: &RubyDocument,
) -> Vec<InlayHintData> {
    let mut hints = Vec::new();

    for node in nodes {
        if let InlayNode::VariableWrite {
            kind,
            name,
            name_end_position,
        } = node
        {
            // Skip constants - they don't get type hints
            if *kind == VariableKind::Constant {
                continue;
            }

            let ruby_type = infer_variable_type(*kind, name, context, document, name_end_position);

            let label = match &ruby_type {
                Some(ty) if *ty != RubyType::Unknown => format!(": {}", ty),
                _ => ": ?".to_string(),
            };

            hints.push(InlayHintData {
                position: *name_end_position,
                label,
                kind: InlayHintKind::VariableType,
                tooltip: None,
                padding_left: false,
                padding_right: false,
            });
        }
    }

    hints
}

/// Generate method return type and parameter hints.
pub fn generate_method_hints(nodes: &[InlayNode], context: &HintContext) -> Vec<InlayHintData> {
    let mut hints = Vec::new();
    let index = context.index.lock();

    // Get method entries for this file
    let entries = index.file_entries(context.uri);

    for node in nodes {
        if let InlayNode::MethodDef {
            name,
            params,
            return_type_position,
            ..
        } = node
        {
            // Find the method entry in the index
            let method_entry = entries.iter().find(|e| {
                if let EntryKind::Method(data) = &e.kind {
                    // Match by name and approximate position
                    data.name.get_name() == *name
                } else {
                    false
                }
            });

            if let Some(entry) = method_entry {
                if let EntryKind::Method(data) = &entry.kind {
                    // Return type hint
                    let return_type_str = data
                        .return_type
                        .as_ref()
                        .map(|rt| rt.to_string())
                        .or_else(|| {
                            data.yard_doc
                                .as_ref()
                                .and_then(|doc| doc.format_return_type())
                        })
                        .unwrap_or_else(|| "?".to_string());

                    hints.push(InlayHintData {
                        position: *return_type_position,
                        label: format!(" -> {}", return_type_str),
                        kind: InlayHintKind::MethodReturn,
                        tooltip: data
                            .yard_doc
                            .as_ref()
                            .and_then(|doc| doc.get_return_description().cloned()),
                        padding_left: false,
                        padding_right: false,
                    });

                    // Parameter type hints from YARD
                    if let Some(yard_doc) = &data.yard_doc {
                        for param in params {
                            if let Some(type_str) = yard_doc.get_param_type_str(&param.name) {
                                let yard_param =
                                    yard_doc.params.iter().find(|p| p.name == param.name);

                                let label = if param.has_colon {
                                    format!(" {}", type_str)
                                } else {
                                    format!(": {}", type_str)
                                };

                                hints.push(InlayHintData {
                                    position: param.end_position,
                                    label,
                                    kind: InlayHintKind::ParameterType,
                                    tooltip: yard_param.and_then(|p| p.description.clone()),
                                    padding_left: false,
                                    padding_right: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    hints
}

/// Generate hints for chained method calls with line breaks.
pub fn generate_chained_call_hints(
    nodes: &[InlayNode],
    _context: &HintContext,
) -> Vec<InlayHintData> {
    let mut hints = Vec::new();

    for node in nodes {
        if let InlayNode::ChainedCall { call_end_position } = node {
            // TODO: Implement proper type inference for chained calls
            // This requires:
            // 1. Inferring the receiver type
            // 2. Looking up the method's return type
            // For now, we'll leave this as a placeholder
            hints.push(InlayHintData {
                position: *call_end_position,
                label: ": ?".to_string(), // Placeholder
                kind: InlayHintKind::ChainedMethodType,
                tooltip: Some("Type at this point in method chain".to_string()),
                padding_left: true,
                padding_right: false,
            });
        }
    }

    // Return empty for now - chained call type inference needs more work
    // Remove this line and return hints when properly implemented
    Vec::new()
}

/// Infer the type of a variable from context.
fn infer_variable_type(
    kind: VariableKind,
    name: &str,
    context: &HintContext,
    document: &RubyDocument,
    position: &Position,
) -> Option<RubyType> {
    match kind {
        VariableKind::Local => {
            // Check document's local variable tracking
            let lvars = document.get_all_lvars();
            for (_scope_id, entries) in lvars {
                for entry in entries {
                    if let EntryKind::LocalVariable(data) = &entry.kind {
                        if data.name == name {
                            // Find the assignment at this specific position
                            // Each assignment has a range, and we want the one where the
                            // variable name ends at our hint position
                            for assignment in &data.assignments {
                                if assignment.range.start.line == position.line {
                                    // Found the assignment at this line
                                    if assignment.r#type != RubyType::Unknown {
                                        return Some(assignment.r#type.clone());
                                    }
                                }
                            }

                            // Fallback: if no assignment matched by position, try document var types
                            if entry.location.range.end == *position {
                                let offset =
                                    crate::utils::position_to_offset(context.content, *position);
                                if let Some(ty) = document.get_var_type(offset, name) {
                                    if *ty != RubyType::Unknown {
                                        return Some(ty.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Try inference from assignment
            let index = context.index.lock();
            crate::query::infer_type_from_assignment(context.content, name, &index)
        }
        VariableKind::Instance | VariableKind::Class | VariableKind::Global => {
            // Look up in index
            let index = context.index.lock();
            let entries = index.file_entries(context.uri);

            for entry in &entries {
                let var_type = match &entry.kind {
                    EntryKind::InstanceVariable(data) if data.name == name => Some(&data.r#type),
                    EntryKind::ClassVariable(data) if data.name == name => Some(&data.r#type),
                    EntryKind::GlobalVariable(data) if data.name == name => Some(&data.r#type),
                    _ => None,
                };

                if let Some(ty) = var_type {
                    if *ty != RubyType::Unknown {
                        return Some(ty.clone());
                    }
                }
            }

            None
        }
        VariableKind::Constant => {
            // Constants don't typically show type hints
            None
        }
    }
}
