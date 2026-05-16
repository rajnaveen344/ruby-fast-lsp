//! Hint generators - Convert collected InlayNodes to InlayHintData.
//!
//! This module contains the logic for generating actual hints from AST nodes.
//! Generators are pure functions that take nodes and context, returning hints.

use super::nodes::{InlayNode, VariableKind};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use ruby_analysis_core::{MethodFact, TypeFact, TypeSubject};
use tower_lsp::lsp_types::Position;

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
    pub content: &'a str,
    pub type_facts: Vec<TypeFact>,
    pub method_facts: Vec<MethodFact>,
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

    for node in nodes {
        if let InlayNode::MethodDef {
            name,
            params,
            return_type_position,
            ..
        } = node
        {
            let return_type_str =
                method_return_type_from_analysis(name, *return_type_position, context)
                    .map(|rt| rt.to_string())
                    .unwrap_or_else(|| "?".to_string());

            hints.push(InlayHintData {
                position: *return_type_position,
                label: format!(" -> {}", return_type_str),
                kind: InlayHintKind::MethodReturn,
                tooltip: None,
                padding_left: false,
                padding_right: false,
            });

            for param in params {
                if let Some(param_type) =
                    parameter_type_from_analysis(name, &param.name, *return_type_position, context)
                {
                    let label = if param.has_colon {
                        format!(" {}", param_type)
                    } else {
                        format!(": {}", param_type)
                    };

                    hints.push(InlayHintData {
                        position: param.end_position,
                        label,
                        kind: InlayHintKind::ParameterType,
                        tooltip: None,
                        padding_left: false,
                        padding_right: false,
                    });
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
            // Try VariableScopes tree
            if let Some(scope_id) = document.variable_scopes().scope_at_position(*position) {
                if let Some(ty) = document
                    .variable_scopes()
                    .get_type_at_position(name, scope_id, *position)
                {
                    if *ty != RubyType::Unknown {
                        return Some(ty.clone());
                    }
                }
            }

            variable_type_from_analysis_facts(kind, name, context, position)
        }
        VariableKind::Instance | VariableKind::Class | VariableKind::Global => {
            if let Some(ty) = variable_type_from_analysis_facts(kind, name, context, position) {
                return Some(ty);
            }
            None
        }
        VariableKind::Constant => {
            // Constants don't typically show type hints
            None
        }
    }
}

fn method_return_type_from_analysis(
    name: &str,
    return_type_position: Position,
    context: &HintContext,
) -> Option<RubyType> {
    let byte_offset = position_to_byte_offset(context.content, return_type_position)?;
    let method_fact = context.method_facts.iter().find(|fact| {
        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            return false;
        };
        method.as_str() == name
            && fact.range.start_byte <= byte_offset
            && byte_offset <= fact.range.end_byte
    })?;

    context
        .type_facts
        .iter()
        .filter_map(|fact| match &fact.subject {
            TypeSubject::MethodReturn(method) if method == &method_fact.fqn => Some(fact),
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_) => None,
        })
        .max_by_key(|fact| fact.range.start_byte)
        .map(|fact| fact.ruby_type.clone())
}

fn parameter_type_from_analysis(
    method_name: &str,
    param_name: &str,
    return_type_position: Position,
    context: &HintContext,
) -> Option<RubyType> {
    let byte_offset = position_to_byte_offset(context.content, return_type_position)?;
    let method_fact = context.method_facts.iter().find(|fact| {
        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            return false;
        };
        method.as_str() == method_name
            && fact.range.start_byte <= byte_offset
            && byte_offset <= fact.range.end_byte
    })?;

    context
        .type_facts
        .iter()
        .filter_map(|fact| match &fact.subject {
            TypeSubject::Parameter { method, name }
                if method == &method_fact.fqn
                    && name == param_name
                    && fact.ruby_type != RubyType::Unknown =>
            {
                Some(fact)
            }
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_) => None,
        })
        .max_by_key(|fact| fact.range.start_byte)
        .map(|fact| fact.ruby_type.clone())
}

fn variable_type_from_analysis_facts(
    kind: VariableKind,
    name: &str,
    context: &HintContext,
    position: &Position,
) -> Option<RubyType> {
    let byte_offset = position_to_byte_offset(context.content, *position)?;
    context
        .type_facts
        .iter()
        .filter(|fact| fact.range.start_byte <= byte_offset)
        .filter_map(|fact| match (&fact.subject, kind) {
            (
                TypeSubject::Local {
                    scope_id: _,
                    name: fact_name,
                },
                VariableKind::Local,
            ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
            (
                TypeSubject::InstanceVariable {
                    name: fact_name, ..
                },
                VariableKind::Instance,
            ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
            (
                TypeSubject::ClassVariable {
                    name: fact_name, ..
                },
                VariableKind::Class,
            ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
            (TypeSubject::GlobalVariable(fact_name), VariableKind::Global)
                if fact_name == name && fact.ruby_type != RubyType::Unknown =>
            {
                Some(fact)
            }
            (
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_),
                _,
            ) => None,
        })
        .max_by_key(|fact| fact.range.start_byte)
        .map(|fact| fact.ruby_type.clone())
}

fn position_to_byte_offset(content: &str, position: Position) -> Option<u32> {
    let mut line = 0u32;
    let mut character = 0u32;

    for (byte_offset, ch) in content.char_indices() {
        if line == position.line && character == position.character {
            return u32::try_from(byte_offset).ok();
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    if line == position.line && character == position.character {
        return u32::try_from(content.len()).ok();
    }

    None
}
