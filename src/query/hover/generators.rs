//! Hover formatting helpers.
//!
//! Ruby target classification and reusable semantic lookup live in
//! `ruby-analysis`; this module builds the protocol-facing hover text.

use parking_lot::Mutex;
use ruby_analysis::core::{NamespaceKind, RubyConstant};
use ruby_analysis::engine::{
    AnalysisEngine, AnalysisQuery, ConstantHover, ConstantHoverKind, VariableTypeKind,
};
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::indexer::{
    resolve_receiver_type, HoverTarget, MethodReceiver, ReceiverResolutionContext,
};
use ruby_analysis::inference::method::method_call_return_type;
use ruby_analysis::inference::RubyType;
use std::sync::Arc;
use tower_lsp::lsp_types::Position;

/// Context for hover generation (provides access to necessary data).
pub struct HoverContext<'a> {
    pub document: Option<&'a Arc<parking_lot::RwLock<RubyDocument>>>,
    pub analysis_engine: Option<&'a Arc<Mutex<AnalysisEngine>>>,
}

/// Hover information for a symbol.
#[derive(Debug, Clone)]
pub struct HoverInfo {
    /// The markdown content to display.
    pub content: String,
    /// The range of the hovered symbol (optional).
    pub range: Option<tower_lsp::lsp_types::Range>,
}

impl HoverInfo {
    /// Create hover info with plain text content.
    pub fn text(content: String) -> Self {
        Self {
            content,
            range: None,
        }
    }

    /// Create hover info formatted as Ruby code block.
    pub fn ruby_code(content: String) -> Self {
        Self {
            content: format!("```ruby\n{}\n```", content),
            range: None,
        }
    }
}

// =============================================================================
// Public Generator Functions
// =============================================================================

/// Generate hover info for a local variable.
pub fn generate_local_variable_hover(
    node: &HoverTarget,
    context: &HoverContext,
) -> Option<HoverInfo> {
    let (name, position, _scope_id) = match node {
        HoverTarget::LocalVariable {
            name,
            position,
            scope_id,
        } => (name, position, scope_id),
        _ => return None,
    };

    // Try VariableScopes tree first (unified type info)
    let from_tree = get_type_from_variable_scopes(context, name, *position);

    // Fall back to TypeQuery (AST-based inference)
    let resolved_type = from_tree.or_else(|| get_type_from_type_query(context, name, *position));

    match resolved_type {
        Some(t) => Some(HoverInfo::text(t.to_string())),
        None => {
            // Check if the variable exists in the tree at all (even with Unknown type)
            let has_variable = context.document.and_then(|doc_arc| {
                let doc = doc_arc.read();
                let scope_id = doc
                    .find_scope_for_variable_at(name, *position)
                    .or_else(|| doc.scope_at_position(*position))?;
                doc.variable_scopes()
                    .find_variable(name, scope_id)
                    .map(|_| ())
            });

            if has_variable.is_some() {
                Some(HoverInfo::text("?".to_string()))
            } else {
                Some(HoverInfo::text(name.to_string()))
            }
        }
    }
}

/// Get type from TypeQuery.
fn get_type_from_type_query(
    context: &HoverContext,
    name: &str,
    position: Position,
) -> Option<RubyType> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    let byte_offset = doc.position_to_analysis_offset(position);
    let scope_id = doc
        .find_scope_for_variable_at(name, position)
        .or_else(|| doc.scope_at_position(position))
        .unwrap_or(0);
    let scope_id = u32::try_from(scope_id).expect(
        "INVARIANT VIOLATED: local variable scope id exceeded u32. \
         This is a bug because analysis TypeSubject stores scope ids as u32. \
         Fix: widen TypeSubject scope ids before storing more than u32::MAX scopes.",
    );
    drop(doc);

    let engine = context.analysis_engine?.lock();
    AnalysisQuery::new(&engine).local_variable_type_at(name, scope_id, file_id, byte_offset)
}

/// Get type from VariableScopes tree (unified type info).
fn get_type_from_variable_scopes(
    context: &HoverContext,
    name: &str,
    position: Position,
) -> Option<RubyType> {
    let doc_arc = context.document?;
    let doc = doc_arc.read();
    let scope_id = doc
        .find_scope_for_variable_at(name, position)
        .or_else(|| doc.scope_at_position(position))?;
    let ty = doc.variable_type_at_position(name, scope_id, position)?;
    if *ty != RubyType::Unknown {
        Some(ty.clone())
    } else {
        None
    }
}

/// Generate hover info for a constant (class/module).
pub fn generate_constant_hover(node: &HoverTarget, context: &HoverContext) -> Option<HoverInfo> {
    let path = match node {
        HoverTarget::Constant { path } => path,
        _ => return None,
    };

    if let Some(hover) = constant_hover_from_analysis(context, path) {
        return Some(hover);
    }
    if context.analysis_engine.is_some() {
        return Some(HoverInfo::text(constant_path_to_string(path)));
    }
    Some(HoverInfo::text(constant_path_to_string(path)))
}

/// Generate hover info for a method (call or definition).
pub fn generate_method_hover(node: &HoverTarget, context: &HoverContext) -> Option<HoverInfo> {
    let (name, position, receiver, namespace, is_definition) = match node {
        HoverTarget::Method {
            name,
            position,
            receiver,
            namespace,
            is_definition,
        } => (name, position, receiver, namespace, is_definition),
        _ => return None,
    };

    // Special handling for .new - return the class instance type
    if name == "new" && !is_definition {
        if let MethodReceiver::Constant(parts) = receiver {
            let fqn_str = parts
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("::");
            return Some(HoverInfo::ruby_code(fqn_str));
        }
    }

    // For method definitions, show inferred/documented return type
    if *is_definition {
        return generate_method_definition_hover(name, *position, context);
    }

    // For method calls, resolve receiver type and infer return type
    let return_type =
        method_call_return_type_from_receiver(context, receiver, name, namespace, *position);

    match return_type {
        Some(t) if t != RubyType::Unknown => Some(HoverInfo::ruby_code(t.to_string())),
        _ => Some(HoverInfo::text("?".to_string())),
    }
}

/// Generate hover info for a variable (instance, class, or global).
pub fn generate_variable_hover(node: &HoverTarget, context: &HoverContext) -> Option<HoverInfo> {
    let (name, variable_kind): (&str, VariableHoverKind) = match node {
        HoverTarget::InstanceVariable { name } => (name.as_str(), VariableHoverKind::Instance),
        HoverTarget::ClassVariable { name } => (name.as_str(), VariableHoverKind::Class),
        HoverTarget::GlobalVariable { name } => (name.as_str(), VariableHoverKind::Global),
        _ => return None,
    };

    if let Some(ruby_type) = variable_type_from_analysis(context, name, variable_kind) {
        return Some(HoverInfo::text(format!("{}: {}", name, ruby_type)));
    }
    if context.analysis_engine.is_some() {
        return Some(HoverInfo::text(name.to_string()));
    }
    Some(HoverInfo::text(name.to_string()))
}

/// Generate hover info for a YARD type reference.
pub fn generate_yard_type_hover(node: &HoverTarget) -> Option<HoverInfo> {
    match node {
        HoverTarget::YardType { type_name } => Some(HoverInfo::text(type_name.clone())),
        _ => None,
    }
}

// =============================================================================
// Private Helpers
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VariableHoverKind {
    Instance,
    Class,
    Global,
}

fn variable_type_kind(kind: VariableHoverKind) -> VariableTypeKind {
    match kind {
        VariableHoverKind::Instance => VariableTypeKind::Instance,
        VariableHoverKind::Class => VariableTypeKind::Class,
        VariableHoverKind::Global => VariableTypeKind::Global,
    }
}

fn variable_type_from_analysis(
    context: &HoverContext,
    name: &str,
    variable_kind: VariableHoverKind,
) -> Option<RubyType> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    drop(doc);

    let engine = context.analysis_engine?.lock();
    AnalysisQuery::new(&engine).variable_type_in_file(
        variable_type_kind(variable_kind),
        name,
        file_id,
    )
}

fn constant_hover_from_analysis(
    context: &HoverContext,
    path: &[RubyConstant],
) -> Option<HoverInfo> {
    let engine = context.analysis_engine?.lock();
    let query = AnalysisQuery::new(&engine);
    query.constant_hover(path).map(format_constant_hover)
}

fn format_constant_hover(hover: ConstantHover) -> HoverInfo {
    match hover.kind {
        ConstantHoverKind::Class => HoverInfo::text(format!("class {}", hover.name)),
        ConstantHoverKind::Module => HoverInfo::text(format!("module {}", hover.name)),
        ConstantHoverKind::Value(ruby_type) => {
            HoverInfo::text(format!("{}: {}", hover.name, ruby_type))
        }
    }
}

fn constant_path_to_string(path: &[RubyConstant]) -> String {
    path.iter()
        .map(|constant| constant.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn method_call_return_type_from_receiver(
    context: &HoverContext,
    receiver: &MethodReceiver,
    method_name: &str,
    namespace: &[RubyConstant],
    position: Position,
) -> Option<RubyType> {
    let doc_guard = context.document.map(|document| document.read());
    let byte_offset = doc_guard
        .as_ref()
        .map(|document| document.position_to_analysis_offset(position))
        .unwrap_or(0);
    let engine_guard = context.analysis_engine.map(|engine| engine.lock());
    let analysis_query = engine_guard
        .as_ref()
        .map(|engine| ruby_analysis::engine::AnalysisQuery::new(engine));

    let resolution_context = ReceiverResolutionContext {
        query: analysis_query.as_ref(),
        document: doc_guard.as_deref(),
        current_namespace: namespace,
        namespace_kind: NamespaceKind::Instance,
        byte_offset,
    };
    let ruby_type = resolve_receiver_type(receiver, &resolution_context);
    if ruby_type == RubyType::Unknown {
        None
    } else {
        method_call_return_type(analysis_query.as_ref(), &ruby_type, method_name)
    }
}

fn generate_method_definition_hover(
    method_name: &str,
    position: Position,
    context: &HoverContext,
) -> Option<HoverInfo> {
    if let Some(hover) = method_definition_hover_from_analysis(method_name, position, context) {
        return Some(hover);
    }
    if context.analysis_engine.is_some() {
        return Some(HoverInfo::ruby_code(format!("def {}", method_name)));
    }
    Some(HoverInfo::ruby_code(format!("def {}", method_name)))
}

fn method_definition_hover_from_analysis(
    method_name: &str,
    position: Position,
    context: &HoverContext,
) -> Option<HoverInfo> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    let byte_offset = doc.position_to_analysis_offset(position);
    drop(doc);

    let engine = context.analysis_engine?.lock();
    let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let return_type = query.method_return_type_at(method_name, file_id, byte_offset)?;
    if return_type == RubyType::Unknown {
        return None;
    }

    Some(HoverInfo::ruby_code(format!(
        "def {} -> {}",
        method_name, return_type
    )))
}
