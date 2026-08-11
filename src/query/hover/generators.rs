//! Hover formatting helpers.
//!
//! Ruby target classification and reusable semantic lookup live in
//! `ruby-analysis`; this module builds the protocol-facing hover text.

use parking_lot::RwLock;
use ruby_analysis::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, UnknownReason,
};
use ruby_analysis::engine::{
    AnalysisEngine, AnalysisQuery, ConstantHover, ConstantHoverKind, VariableTypeKind,
};
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::indexer::{
    resolve_receiver_type, HoverTarget, MethodReceiver, ReceiverResolutionContext,
};
use ruby_analysis::inference::method::method_call_return_type_with_visibility;
use ruby_analysis::inference::RubyType;
use ruby_analysis::yard::YardParser;
use std::sync::Arc;
use tower_lsp::lsp_types::Position;

/// Context for hover generation (provides access to necessary data).
pub struct HoverContext<'a> {
    pub document: Option<&'a Arc<parking_lot::RwLock<RubyDocument>>>,
    pub analysis_engine: Option<&'a Arc<RwLock<AnalysisEngine>>>,
    pub current_namespace: &'a [RubyConstant],
    pub namespace_kind: NamespaceKind,
    pub position: Position,
    pub byte_offset: u32,
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
    let (name, byte_offset, _scope_id) = match node {
        HoverTarget::LocalVariable {
            name,
            byte_offset,
            scope_id,
        } => (name, byte_offset, scope_id),
        _ => return None,
    };

    if let Some(ruby_type) = local_read_type_from_analysis(context, *byte_offset) {
        return Some(HoverInfo::text(ruby_type.to_string()));
    }

    // A flow-owned Unknown is an explicit proof barrier, not absence. Never
    // fall through to an older assignment fact when the forward solver has
    // already established that at least one reaching path is unresolved.
    match get_type_from_variable_scopes(context, name, *byte_offset) {
        Some(RubyType::Unknown) => {
            if let Some((RubyType::Unknown, Some(reason))) =
                expression_type_from_analysis(context, *byte_offset)
            {
                return Some(HoverInfo::text(format_unknown_type(reason)));
            }
            return Some(HoverInfo::text("?".to_string()));
        }
        Some(ruby_type) => return Some(HoverInfo::text(ruby_type.to_string())),
        None => {}
    }

    // Fall back for local forms that do not yet publish exact flow evidence.
    if let Some(ruby_type) = get_type_from_type_query(context, name, *byte_offset)
        .filter(|ruby_type| *ruby_type != RubyType::Unknown)
    {
        return Some(HoverInfo::text(ruby_type.to_string()));
    }

    if let Some((RubyType::Unknown, Some(reason))) =
        expression_type_from_analysis(context, *byte_offset)
    {
        return Some(HoverInfo::text(format_unknown_type(reason)));
    }

    // Check if the variable exists in the tree at all (even with Unknown type)
    let has_variable = context.document.and_then(|doc_arc| {
        let doc = doc_arc.read();
        let position = doc.offset_to_position(*byte_offset as usize);
        let scope_id = doc
            .find_scope_for_variable_at(name, position)
            .or_else(|| doc.scope_at_position(position))?;
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

fn local_read_type_from_analysis(context: &HoverContext, byte_offset: u32) -> Option<RubyType> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    drop(doc);

    let engine = context.analysis_engine?.read();
    AnalysisQuery::new(&engine).local_read_type_at(file_id, byte_offset)
}

/// Get type from TypeQuery.
fn get_type_from_type_query(
    context: &HoverContext,
    name: &str,
    byte_offset: u32,
) -> Option<RubyType> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    let position = doc.offset_to_position(byte_offset as usize);
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

    let engine = context.analysis_engine?.read();
    AnalysisQuery::new(&engine).local_variable_type_at(name, scope_id, file_id, byte_offset)
}

/// Get type from VariableScopes tree (unified type info).
fn get_type_from_variable_scopes(
    context: &HoverContext,
    name: &str,
    byte_offset: u32,
) -> Option<RubyType> {
    let doc_arc = context.document?;
    let doc = doc_arc.read();
    let position = doc.offset_to_position(byte_offset as usize);
    let scope_id = doc
        .find_scope_for_variable_at(name, position)
        .or_else(|| doc.scope_at_position(position))?;
    doc.variable_type_at_position(name, scope_id, position)
        .cloned()
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
    let (name, byte_offset, receiver, namespace, namespace_kind, is_definition, has_call_result) =
        match node {
            HoverTarget::Method {
                name,
                byte_offset,
                receiver,
                namespace,
                namespace_kind,
                is_definition,
                has_call_result,
            } => (
                name,
                byte_offset,
                receiver,
                namespace,
                namespace_kind,
                is_definition,
                has_call_result,
            ),
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
        let definition_namespace_kind = if !matches!(receiver, MethodReceiver::None) {
            NamespaceKind::Singleton
        } else {
            *namespace_kind
        };
        return generate_method_definition_hover(
            name,
            namespace,
            definition_namespace_kind,
            *byte_offset,
            context.position,
            context,
        );
    }

    // A file-owned call outcome is the shared solver's final proof decision.
    // Protocol adapters must never replace an exact Unknown with a second,
    // independently resolved concrete type.
    if *has_call_result {
        match call_expression_outcome_from_analysis(context, *byte_offset)
            .or_else(|| expression_type_from_analysis(context, *byte_offset))
        {
            Some((RubyType::Unknown, unknown_reason)) => {
                return Some(HoverInfo::text(
                    unknown_reason
                        .map(format_unknown_type)
                        .unwrap_or_else(|| "?".to_string()),
                ));
            }
            Some((ruby_type, None)) => {
                return Some(HoverInfo::ruby_code(ruby_type.to_string()));
            }
            Some((ruby_type, Some(reason))) => panic!(
                "INVARIANT VIOLATED: concrete call type `{ruby_type}` carried Unknown reason `{}`. This is a bug because proof-failure evidence belongs only to RubyType::Unknown. Fix: attach expression reasons only when the exact call outcome is Unknown.",
                reason.code()
            ),
            None => {}
        }
    }

    // For method calls, resolve receiver type and infer return type
    let return_type = method_call_return_type_from_receiver(
        context,
        receiver,
        name,
        namespace,
        *namespace_kind,
        *byte_offset,
    );

    match return_type {
        Some(t) if t != RubyType::Unknown => Some(HoverInfo::ruby_code(t.to_string())),
        Some(RubyType::Unknown) | None => Some(HoverInfo::text("?".to_string())),
        Some(t) => panic!(
            "INVARIANT VIOLATED: method hover matched an unhandled concrete return type `{t}`. This is a bug because the preceding guard accepts every non-Unknown RubyType. Fix: keep hover return projection exhaustive."
        ),
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

    if let Some((ruby_type, unknown_reason)) =
        variable_type_from_analysis(context, name, variable_kind)
    {
        return Some(HoverInfo::text(match (ruby_type, unknown_reason) {
            (RubyType::Unknown, Some(reason)) => {
                format!("{}: {}", name, format_unknown_type(reason))
            }
            (RubyType::Unknown, None) => format!("{}: ?", name),
            (ruby_type, None) => format!("{}: {}", name, ruby_type),
            (ruby_type, Some(reason)) => panic!(
                "INVARIANT VIOLATED: concrete variable type `{ruby_type}` carried Unknown reason `{}`. This is a bug because proof-failure evidence belongs only to RubyType::Unknown. Fix: return a reason only when the exact expression type is Unknown.",
                reason.code()
            ),
        }));
    }
    if context.analysis_engine.is_some() {
        return Some(HoverInfo::text(format!("{}: ?", name)));
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
) -> Option<(RubyType, Option<UnknownReason>)> {
    if let Some(outcome) = expression_type_from_analysis(context, context.byte_offset) {
        return Some(outcome);
    }

    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    let byte_offset = context.byte_offset;
    drop(doc);

    let engine = context.analysis_engine?.read();
    let query = AnalysisQuery::new(&engine);
    let owner = FullyQualifiedName::namespace_with_kind(
        context.current_namespace.to_vec(),
        context.namespace_kind,
    );
    query
        .variable_type_before_in_owner(
            variable_type_kind(variable_kind),
            name,
            &owner,
            file_id,
            byte_offset,
        )
        .map(|ruby_type| (ruby_type, None))
}

fn expression_type_from_analysis(
    context: &HoverContext,
    byte_offset: u32,
) -> Option<(RubyType, Option<UnknownReason>)> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    drop(doc);

    let engine = context.analysis_engine?.read();
    let query = AnalysisQuery::new(&engine);
    match query.expression_type_at(file_id, byte_offset) {
        Some(ruby_type) if ruby_type != RubyType::Unknown => Some((ruby_type, None)),
        Some(RubyType::Unknown) => Some((
            RubyType::Unknown,
            query.expression_unknown_reason_at(file_id, byte_offset),
        )),
        None => query
            .expression_unknown_reason_at(file_id, byte_offset)
            .map(|reason| (RubyType::Unknown, Some(reason))),
        Some(ruby_type) => panic!(
            "INVARIANT VIOLATED: concrete expression type `{ruby_type}` bypassed the concrete hover branch. This is a bug because the first match arm accepts every non-Unknown RubyType. Fix: keep expression hover projection exhaustive."
        ),
    }
}

fn call_expression_outcome_from_analysis(
    context: &HoverContext,
    byte_offset: u32,
) -> Option<(RubyType, Option<UnknownReason>)> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    drop(doc);

    let engine = context.analysis_engine?.read();
    let query = AnalysisQuery::new(&engine);
    let outcome = query.call_expression_outcome_at_position(file_id, byte_offset)?;
    match (outcome.proven_type(), outcome.unknown_reason()) {
        (Some(ruby_type), None) => Some((ruby_type.clone(), None)),
        (None, Some(reason)) => Some((RubyType::Unknown, Some(reason))),
        (Some(ruby_type), Some(reason)) => panic!(
            "INVARIANT VIOLATED: proven call type `{ruby_type}` carried Unknown reason `{}`. This is a bug because TypeInferenceOutcome must represent exactly one proof state. Fix: construct call outcomes through TypeInferenceOutcome::proven or TypeInferenceOutcome::unknown.",
            reason.code()
        ),
        (None, None) => panic!(
            "INVARIANT VIOLATED: a call inference outcome has neither a proven type nor an Unknown reason. This is a bug because TypeInferenceOutcome must represent exactly one proof state. Fix: keep its state representation exhaustive."
        ),
    }
}

fn format_unknown_type(reason: UnknownReason) -> String {
    format!("?\nUnknown[{}]: {}", reason.code(), reason.explanation())
}

fn constant_hover_from_analysis(
    context: &HoverContext,
    path: &[RubyConstant],
) -> Option<HoverInfo> {
    let engine = context.analysis_engine?.read();
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
    namespace_kind: NamespaceKind,
    byte_offset: u32,
) -> Option<RubyType> {
    if receiver == &MethodReceiver::Super {
        return super_method_return_type_from_analysis(
            context,
            method_name,
            namespace,
            namespace_kind,
        );
    }

    let doc_guard = context.document.map(|document| document.read());
    let byte_offset = doc_guard.as_ref().map(|_| byte_offset).unwrap_or(0);
    let method_return_type = {
        let engine_guard = context.analysis_engine.map(|engine| engine.read());
        let analysis_query = engine_guard
            .as_ref()
            .map(|engine| ruby_analysis::engine::AnalysisQuery::new(engine));

        let resolution_context = ReceiverResolutionContext {
            query: analysis_query.as_ref(),
            document: doc_guard.as_deref(),
            current_namespace: namespace,
            namespace_kind,
            byte_offset,
        };
        let ruby_type = resolve_receiver_type(receiver, &resolution_context);
        if ruby_type == RubyType::Unknown {
            None
        } else {
            let allow_private = matches!(receiver, MethodReceiver::None)
                || doc_guard.as_ref().is_some_and(|document| {
                    static_send_symbol_at_position(&document.content, context.position)
                });
            let caller_namespace =
                FullyQualifiedName::namespace_with_kind(namespace.to_vec(), namespace_kind);
            let protected_caller = (!allow_private).then_some(&caller_namespace);
            method_call_return_type_with_visibility(
                analysis_query.as_ref(),
                &ruby_type,
                method_name,
                allow_private,
                protected_caller,
            )
        }
    };

    method_return_type.filter(|return_type| *return_type != RubyType::Unknown)
}

fn static_send_symbol_at_position(content: &str, position: Position) -> bool {
    let Some(line) = content.lines().nth(position.line as usize) else {
        return false;
    };
    line.contains(".send(:")
        || line.contains(".__send__(:")
        || line.contains(".send(\"")
        || line.contains(".__send__(\"")
}

fn super_method_return_type_from_analysis(
    context: &HoverContext,
    method_name: &str,
    namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
) -> Option<RubyType> {
    let method = RubyMethod::new(method_name).ok()?;
    let engine = context.analysis_engine?.read();
    let query = AnalysisQuery::new(&engine);
    let owner = FullyQualifiedName::namespace_with_kind(namespace.to_vec(), namespace_kind);
    let callee = query.resolve_super_method_callee(&owner, &method)?;
    query.method_return_type_for_receiver(&callee.owner, &method)
}

fn generate_method_definition_hover(
    method_name: &str,
    namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    byte_offset: u32,
    position: Position,
    context: &HoverContext,
) -> Option<HoverInfo> {
    if let Some(hover) = method_definition_hover_from_analysis(
        method_name,
        namespace,
        namespace_kind,
        byte_offset,
        context,
    ) {
        return Some(hover);
    }
    if let Some(hover) = method_definition_hover_from_yard(method_name, position, context) {
        return Some(hover);
    }
    if context.analysis_engine.is_some() {
        return Some(HoverInfo::ruby_code(format!("def {}", method_name)));
    }
    Some(HoverInfo::ruby_code(format!("def {}", method_name)))
}

fn method_definition_hover_from_yard(
    method_name: &str,
    position: Position,
    context: &HoverContext,
) -> Option<HoverInfo> {
    let doc = context.document?.read();
    let content = doc.content.clone();
    drop(doc);

    let method_start_offset = line_start_byte_offset(&content, position.line)?;
    let doc = YardParser::extract_from_source(&content, method_start_offset)?;
    let return_type = doc
        .returns
        .iter()
        .flat_map(|return_doc| return_doc.types.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if return_type.is_empty() {
        return None;
    }
    Some(HoverInfo::ruby_code(format!(
        "def {} -> {}",
        method_name, return_type
    )))
}

fn line_start_byte_offset(content: &str, target_line: u32) -> Option<usize> {
    let mut offset = 0;
    for (line_idx, line) in content.lines().enumerate() {
        if line_idx as u32 == target_line {
            return Some(offset);
        }
        offset += line.len() + 1;
    }
    None
}

fn method_definition_hover_from_analysis(
    method_name: &str,
    namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    byte_offset: u32,
    context: &HoverContext,
) -> Option<HoverInfo> {
    let doc = context.document?.read();
    let file_id = doc.analysis_file_id();
    drop(doc);

    let engine = context.analysis_engine?.read();
    let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let return_type = query
        .method_return_type_at_with_kind(method_name, namespace_kind, file_id, byte_offset)
        .or_else(|| {
            let method = RubyMethod::new(method_name).ok()?;
            let owner = FullyQualifiedName::namespace_with_kind(namespace.to_vec(), namespace_kind);
            query.method_return_type_for_receiver(&owner, &method)
        })?;
    if return_type == RubyType::Unknown {
        return None;
    }

    Some(HoverInfo::ruby_code(format!(
        "def {} -> {}",
        method_name, return_type
    )))
}
