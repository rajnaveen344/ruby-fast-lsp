//! Hint generators - Convert collected InlayNodes to InlayHintData.
//!
//! This module contains the logic for generating actual hints from AST nodes.
//! Generators are pure functions that take nodes and context, returning hints.

use super::navigation::type_hint_label;
use crate::utils::lsp::lsp_position;
use ruby_analysis::core::RubyType;
use ruby_analysis::core::{SourceFileId, VariableTypeKind};
use ruby_analysis::engine::View;
use ruby_analysis::indexer::{
    inlay_hints::{InlayNode, VariableKind},
    RubyDocument,
};
use tower_lsp::lsp_types::{InlayHintLabel, InlayHintTooltip, Position};

/// Unified inlay hint data structure.
#[derive(Debug, Clone)]
pub struct InlayHintData {
    pub position: Position,
    pub label: InlayHintLabel,
    pub kind: InlayHintKind,
    pub tooltip: Option<InlayHintTooltip>,
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

/// Context for hint generation: the document and one view of its engine.
pub struct HintContext<'a> {
    pub file_id: SourceFileId,
    pub document: &'a RubyDocument,
    pub view: &'a View<'a>,
}

impl HintContext<'_> {
    fn type_label(&self, ruby_type: &RubyType, prefix: &str) -> (InlayHintLabel, InlayHintTooltip) {
        type_hint_label(ruby_type, prefix, self.view)
    }
}

/// Generate structural hints (end labels, implicit returns).
///
/// These hints don't require type inference.
pub fn generate_structural_hints(nodes: &[InlayNode], context: &HintContext) -> Vec<InlayHintData> {
    nodes
        .iter()
        .filter_map(|node| match node {
            InlayNode::BlockEnd {
                kind,
                name,
                end_offset,
            } => Some(InlayHintData {
                position: lsp_position(context.document.offset_to_position(*end_offset as usize)),
                label: InlayHintLabel::String(format!("{} {}", kind.keyword(), name)),
                kind: InlayHintKind::EndLabel,
                tooltip: None,
                padding_left: true,
                padding_right: false,
            }),
            InlayNode::ImplicitReturn { offset } => Some(InlayHintData {
                position: lsp_position(context.document.offset_to_position(*offset as usize)),
                label: InlayHintLabel::String("return".to_string()),
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
/// Omits constants whose values do not have a proven type.
pub fn generate_variable_type_hints(
    nodes: &[InlayNode],
    context: &HintContext,
) -> Vec<InlayHintData> {
    let mut hints = Vec::new();

    for node in nodes {
        if let InlayNode::VariableWrite {
            kind,
            name,
            name_start_offset,
            name_end_offset,
        } = node
        {
            let ruby_type =
                infer_variable_type(*kind, name, context, *name_start_offset, *name_end_offset);

            // Value constants are typed-only: skip Unknown so dynamic RHS stays quiet.
            // Locals/ivars keep the existing ": ?" placeholder.
            let ruby_type = ruby_type.unwrap_or(RubyType::Unknown);
            if *kind == VariableKind::Constant && ruby_type == RubyType::Unknown {
                continue;
            }
            let (label, tooltip) = context.type_label(&ruby_type, ": ");

            hints.push(InlayHintData {
                position: lsp_position(
                    context
                        .document
                        .offset_to_position(*name_end_offset as usize),
                ),
                label,
                kind: InlayHintKind::VariableType,
                tooltip: Some(tooltip),
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
            return_type_offset,
            ..
        } = node
        {
            let return_type = method_return_type_from_analysis(name, *return_type_offset, context)
                .unwrap_or(RubyType::Unknown);
            let (label, tooltip) = context.type_label(&return_type, " -> ");

            hints.push(InlayHintData {
                position: lsp_position(
                    context
                        .document
                        .offset_to_position(*return_type_offset as usize),
                ),
                label,
                kind: InlayHintKind::MethodReturn,
                tooltip: Some(tooltip),
                padding_left: false,
                padding_right: false,
            });

            for param in params {
                if let Some(param_type) =
                    parameter_type_from_analysis(name, &param.name, *return_type_offset, context)
                {
                    let prefix = if param.has_colon { " " } else { ": " };
                    let (label, tooltip) = context.type_label(&param_type, prefix);

                    hints.push(InlayHintData {
                        position: lsp_position(
                            context
                                .document
                                .offset_to_position(param.end_offset as usize),
                        ),
                        label,
                        kind: InlayHintKind::ParameterType,
                        tooltip: Some(tooltip),
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
    context: &HintContext,
) -> Vec<InlayHintData> {
    let mut hints = Vec::new();

    for node in nodes {
        if let InlayNode::ChainedCall { call_end_offset } = node {
            let Some(ruby_type) = context
                .view
                .proven_expression_type_ending_at(context.file_id, *call_end_offset)
            else {
                continue;
            };
            let (label, tooltip) = context.type_label(&ruby_type, ": ");
            hints.push(InlayHintData {
                position: lsp_position(
                    context
                        .document
                        .offset_to_position(*call_end_offset as usize),
                ),
                label,
                kind: InlayHintKind::ChainedMethodType,
                tooltip: Some(tooltip),
                padding_left: true,
                padding_right: false,
            });
        }
    }

    hints
}

/// Infer the type of a variable from context.
fn infer_variable_type(
    kind: VariableKind,
    name: &str,
    context: &HintContext,
    name_start_offset: u32,
    name_end_offset: u32,
) -> Option<RubyType> {
    match kind {
        VariableKind::Local => {
            // Exact engine facts are the semantic authority after deferred
            // cross-file equations resolve. The open document's VariableScopes
            // snapshot may predate cold indexing, so it is only a fallback for
            // local forms that do not publish an exact assignment fact yet.
            if let Some(ty) = variable_type_from_analysis_facts(
                kind,
                name,
                context,
                name_start_offset,
                name_end_offset,
            )
            .filter(|ty| *ty != RubyType::Unknown)
            {
                return Some(ty);
            }

            let position = context
                .document
                .offset_to_position(name_end_offset as usize);

            if let Some(scope_id) = context.document.scope_at_position(position) {
                if let Some(ty) = context
                    .document
                    .variable_type_at_position(name, scope_id, position)
                {
                    if *ty != RubyType::Unknown {
                        return Some(ty.clone());
                    }
                }
            }
            None
        }
        VariableKind::Instance
        | VariableKind::Class
        | VariableKind::Global
        | VariableKind::Constant => {
            if let Some(ty) = variable_type_from_analysis_facts(
                kind,
                name,
                context,
                name_start_offset,
                name_end_offset,
            ) {
                return Some(ty);
            }
            None
        }
    }
}

fn method_return_type_from_analysis(
    name: &str,
    byte_offset: u32,
    context: &HintContext,
) -> Option<RubyType> {
    context
        .view
        .method_return_type_at(name, context.file_id, byte_offset)
}

fn parameter_type_from_analysis(
    method_name: &str,
    param_name: &str,
    byte_offset: u32,
    context: &HintContext,
) -> Option<RubyType> {
    context
        .view
        .parameter_type_at(method_name, param_name, context.file_id, byte_offset)
}

fn variable_type_from_analysis_facts(
    kind: VariableKind,
    name: &str,
    context: &HintContext,
    name_start_offset: u32,
    name_end_offset: u32,
) -> Option<RubyType> {
    context.view.variable_assignment_type_at(
        variable_type_kind(kind),
        name,
        context.file_id,
        name_start_offset,
        name_end_offset,
    )
}

fn variable_type_kind(kind: VariableKind) -> VariableTypeKind {
    match kind {
        VariableKind::Local => VariableTypeKind::Local,
        VariableKind::Instance => VariableTypeKind::Instance,
        VariableKind::Class => VariableTypeKind::Class,
        VariableKind::Global => VariableTypeKind::Global,
        VariableKind::Constant => VariableTypeKind::Constant,
    }
}
