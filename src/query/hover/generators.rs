//! Hover generators - convert HoverNodes to HoverInfo.
//!
//! Each generator function is a pure function that takes a node and context,
//! and returns formatted hover information.

use super::nodes::HoverNode;
use parking_lot::Mutex;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::engine::{
    AnalysisEngine, AnalysisQuery, ConstantHover, ConstantHoverKind, VariableTypeKind,
};
use ruby_analysis::indexer::LVScopeId;
use ruby_analysis::indexer::MethodReceiver;
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::inference::RubyType;
use std::sync::Arc;
use tower_lsp::lsp_types::Position;

/// Context for hover generation (provides access to necessary data).
pub struct HoverContext<'a> {
    pub content: &'a str,
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
    node: &HoverNode,
    context: &HoverContext,
) -> Option<HoverInfo> {
    let (name, position, _scope_id) = match node {
        HoverNode::LocalVariable {
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

    let byte_offset = position_to_byte_offset(context.content, position)?;
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
pub fn generate_constant_hover(node: &HoverNode, context: &HoverContext) -> Option<HoverInfo> {
    let path = match node {
        HoverNode::Constant { path } => path,
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
pub fn generate_method_hover(node: &HoverNode, context: &HoverContext) -> Option<HoverInfo> {
    let (name, position, receiver, namespace, scope_id, is_definition) = match node {
        HoverNode::Method {
            name,
            position,
            receiver,
            namespace,
            scope_id,
            is_definition,
        } => (name, position, receiver, namespace, scope_id, is_definition),
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
    let receiver_type = resolve_receiver_type(receiver, namespace, *scope_id, *position, context);

    let return_type = method_call_return_type(context, &receiver_type, name);

    match return_type {
        Some(t) if t != RubyType::Unknown => Some(HoverInfo::ruby_code(t.to_string())),
        _ => Some(HoverInfo::text("?".to_string())),
    }
}

/// Generate hover info for a variable (instance, class, or global).
pub fn generate_variable_hover(node: &HoverNode, context: &HoverContext) -> Option<HoverInfo> {
    let (name, variable_kind): (&str, VariableHoverKind) = match node {
        HoverNode::InstanceVariable { name } => (name.as_str(), VariableHoverKind::Instance),
        HoverNode::ClassVariable { name } => (name.as_str(), VariableHoverKind::Class),
        HoverNode::GlobalVariable { name } => (name.as_str(), VariableHoverKind::Global),
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
pub fn generate_yard_type_hover(node: &HoverNode) -> Option<HoverInfo> {
    match node {
        HoverNode::YardType { type_name } => Some(HoverInfo::text(type_name.clone())),
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

fn namespace_type_from_analysis(
    context: &HoverContext,
    namespace_fqn: &FullyQualifiedName,
) -> Option<RubyType> {
    let engine = context.analysis_engine?.lock();
    AnalysisQuery::new(&engine).namespace_type(namespace_fqn)
}

fn constant_reference_type_from_analysis(
    context: &HoverContext,
    path: &[RubyConstant],
) -> Option<RubyType> {
    let engine = context.analysis_engine?.lock();
    AnalysisQuery::new(&engine).constant_reference_type(path)
}

fn method_call_return_type(
    context: &HoverContext,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    use ruby_analysis::core::RubyMethod;

    if method_name == "new" {
        if let RubyType::ClassReference(fqn) = receiver_type {
            return Some(RubyType::Class(fqn.clone()));
        }
    }

    if let Some(return_type) = generic_rbs_method_return_type(receiver_type, method_name) {
        return Some(return_type);
    }

    let method = RubyMethod::new(method_name).ok()?;
    if let Some(engine) = context.analysis_engine {
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        for namespace in query.receiver_type_to_method_namespaces(receiver_type) {
            if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
                return Some(return_type);
            }
        }
    }

    rbs_method_return_type(receiver_type, method_name)
}

fn generic_rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Array(element_types) => {
            ruby_analysis::inference::rbs::get_rbs_method_return_type_with_type_args(
                "Array",
                method_name,
                false,
                element_types,
            )
        }
        RubyType::Hash(key_types, value_types) => {
            let type_args = vec![
                RubyType::union(key_types.clone()),
                RubyType::union(value_types.clone()),
            ];
            ruby_analysis::inference::rbs::get_rbs_method_return_type_with_type_args(
                "Hash",
                method_name,
                false,
                &type_args,
            )
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}

fn rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) => {
            generic_rbs_method_return_type(receiver_type, method_name)
        }
        RubyType::Union(types) => {
            let mut return_types = types
                .iter()
                .filter_map(|ty| {
                    generic_rbs_method_return_type(ty, method_name)
                        .or_else(|| rbs_method_return_type(ty, method_name))
                })
                .collect::<Vec<_>>();
            return_types.sort_by_key(|ty| ty.to_string());
            return_types.dedup();
            match return_types.len() {
                0 => None,
                1 => return_types.pop(),
                _ => Some(RubyType::union(return_types)),
            }
        }
        RubyType::Unknown => None,
    }
}

fn rbs_method_return_for_fqn(
    fqn: &FullyQualifiedName,
    method_name: &str,
    is_singleton: bool,
) -> Option<RubyType> {
    for class_name in class_names_for_fqn(fqn) {
        if let Some(return_type) =
            ruby_analysis::inference::rbs::get_rbs_method_return_type_as_ruby_type(
                &class_name,
                method_name,
                is_singleton,
            )
        {
            return Some(return_type);
        }
    }
    None
}

fn class_names_for_fqn(fqn: &FullyQualifiedName) -> Vec<String> {
    let parts = fqn.namespace_parts();
    let fqn_name = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("::");
    let simple_name = parts.last().map(|part| part.to_string());

    let mut names = Vec::new();
    if !fqn_name.is_empty() {
        names.push(fqn_name);
    }
    if let Some(simple_name) = simple_name {
        if !names.contains(&simple_name) {
            names.push(simple_name);
        }
    }
    names
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
    drop(doc);

    let byte_offset = position_to_byte_offset(context.content, position)?;
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

fn resolve_receiver_type(
    receiver: &MethodReceiver,
    namespace: &[RubyConstant],
    scope_id: LVScopeId,
    position: Position,
    context: &HoverContext,
) -> RubyType {
    match receiver {
        MethodReceiver::None | MethodReceiver::SelfReceiver => {
            if namespace.is_empty() {
                RubyType::class("Object")
            } else {
                let fqn = FullyQualifiedName::from(namespace.to_vec());
                if let Some(ruby_type) = namespace_type_from_analysis(context, &fqn) {
                    return ruby_type;
                }
                if context.analysis_engine.is_some() {
                    return RubyType::Class(fqn);
                }
                RubyType::Class(fqn)
            }
        }
        MethodReceiver::Constant(path) => {
            if let Some(ruby_type) = constant_reference_type_from_analysis(context, path) {
                return ruby_type;
            }
            let fqn = FullyQualifiedName::Constant(path.clone());
            RubyType::ClassReference(fqn)
        }
        MethodReceiver::LocalVariable(name) => {
            // Try VariableScopes tree first
            if let Some(t) = get_type_from_variable_scopes(context, name, position) {
                return t;
            }

            // Fall back to analysis facts.
            if let Some(t) = get_type_from_type_query(context, name, position) {
                return t;
            }

            RubyType::Unknown
        }
        MethodReceiver::InstanceVariable(name) => {
            if let Some(ruby_type) =
                variable_type_from_analysis(context, name, VariableHoverKind::Instance)
            {
                return ruby_type;
            }
            RubyType::Unknown
        }
        MethodReceiver::ClassVariable(name) => {
            if let Some(ruby_type) =
                variable_type_from_analysis(context, name, VariableHoverKind::Class)
            {
                return ruby_type;
            }
            RubyType::Unknown
        }
        MethodReceiver::GlobalVariable(name) => {
            if let Some(ruby_type) =
                variable_type_from_analysis(context, name, VariableHoverKind::Global)
            {
                return ruby_type;
            }
            RubyType::Unknown
        }
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => {
            // Special handling for .new on constants
            if method_name == "new" {
                if let MethodReceiver::Constant(path) = inner_receiver.as_ref() {
                    let fqn = FullyQualifiedName::Constant(path.clone());
                    return RubyType::Class(fqn);
                }
            }

            let inner_type =
                resolve_receiver_type(inner_receiver, namespace, scope_id, position, context);

            if inner_type == RubyType::Unknown {
                return RubyType::Unknown;
            }

            method_call_return_type(context, &inner_type, method_name).unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::Literal(t) => t.clone(),
        MethodReceiver::Expression => RubyType::Unknown,
    }
}
