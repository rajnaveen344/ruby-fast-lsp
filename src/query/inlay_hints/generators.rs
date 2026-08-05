//! Hint generators - Convert collected InlayNodes to InlayHintData.
//!
//! This module contains the logic for generating actual hints from AST nodes.
//! Generators are pure functions that take nodes and context, returning hints.

use parking_lot::RwLock;
use ruby_analysis::core::SourceFileId;
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, VariableTypeKind};
use ruby_analysis::indexer::{
    inlay_hints::{InlayNode, VariableKind},
    RubyDocument,
};
use ruby_analysis::inference::RubyType;
use std::sync::Arc;
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
    pub file_id: SourceFileId,
    pub document: &'a RubyDocument,
    pub analysis_engine: Option<Arc<RwLock<AnalysisEngine>>>,
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
                position: context.document.offset_to_position(*end_offset as usize),
                label: format!("{} {}", kind.keyword(), name),
                kind: InlayHintKind::EndLabel,
                tooltip: None,
                padding_left: true,
                padding_right: false,
            }),
            InlayNode::ImplicitReturn { offset } => Some(InlayHintData {
                position: context.document.offset_to_position(*offset as usize),
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
            let label = match (&ruby_type, kind) {
                (Some(ty), VariableKind::Constant) if *ty != RubyType::Unknown => {
                    format!(": {}", ty)
                }
                (_, VariableKind::Constant) => continue,
                (Some(ty), _) if *ty != RubyType::Unknown => format!(": {}", ty),
                (_, _) => ": ?".to_string(),
            };

            hints.push(InlayHintData {
                position: context
                    .document
                    .offset_to_position(*name_end_offset as usize),
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
            return_type_offset,
            ..
        } = node
        {
            let return_type_str =
                method_return_type_from_analysis(name, *return_type_offset, context)
                    .map(|rt| rt.to_string())
                    .unwrap_or_else(|| "?".to_string());

            hints.push(InlayHintData {
                position: context
                    .document
                    .offset_to_position(*return_type_offset as usize),
                label: format!(" -> {}", return_type_str),
                kind: InlayHintKind::MethodReturn,
                tooltip: None,
                padding_left: false,
                padding_right: false,
            });

            for param in params {
                if let Some(param_type) =
                    parameter_type_from_analysis(name, &param.name, *return_type_offset, context)
                {
                    let label = if param.has_colon {
                        format!(" {}", param_type)
                    } else {
                        format!(": {}", param_type)
                    };

                    hints.push(InlayHintData {
                        position: context
                            .document
                            .offset_to_position(param.end_offset as usize),
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
    context: &HintContext,
) -> Vec<InlayHintData> {
    let Some(engine) = context.analysis_engine.as_ref() else {
        return Vec::new();
    };
    let engine = engine.read();
    let query = AnalysisQuery::new(&engine);
    let mut hints = Vec::new();

    for node in nodes {
        if let InlayNode::ChainedCall { call_end_offset } = node {
            let Some(ruby_type) =
                query.proven_expression_type_ending_at(context.file_id, *call_end_offset)
            else {
                continue;
            };
            hints.push(InlayHintData {
                position: context
                    .document
                    .offset_to_position(*call_end_offset as usize),
                label: format!(": {ruby_type}"),
                kind: InlayHintKind::ChainedMethodType,
                tooltip: Some("Proven intermediate type in method chain".to_string()),
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
            let position = context
                .document
                .offset_to_position(name_end_offset as usize);

            // Try VariableScopes tree
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

            variable_type_from_analysis_facts(
                kind,
                name,
                context,
                name_start_offset,
                name_end_offset,
            )
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
    let engine = context.analysis_engine.as_ref()?;
    let engine = engine.read();
    AnalysisQuery::new(&engine).method_return_type_at(name, context.file_id, byte_offset)
}

fn parameter_type_from_analysis(
    method_name: &str,
    param_name: &str,
    byte_offset: u32,
    context: &HintContext,
) -> Option<RubyType> {
    let engine = context.analysis_engine.as_ref()?;
    let engine = engine.read();
    AnalysisQuery::new(&engine).parameter_type_at(
        method_name,
        param_name,
        context.file_id,
        byte_offset,
    )
}

fn variable_type_from_analysis_facts(
    kind: VariableKind,
    name: &str,
    context: &HintContext,
    name_start_offset: u32,
    name_end_offset: u32,
) -> Option<RubyType> {
    let engine = context.analysis_engine.as_ref()?;
    let engine = engine.read();
    AnalysisQuery::new(&engine).variable_assignment_type_at(
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
