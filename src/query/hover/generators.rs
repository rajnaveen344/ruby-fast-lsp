//! Hover generators - convert HoverNodes to HoverInfo.
//!
//! Each generator function is a pure function that takes a node and context,
//! and returns formatted hover information.

use super::nodes::HoverNode;
use crate::analyzer_prism::MethodReceiver;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::return_type::infer_return_type_for_node;
use crate::query::TypeQuery;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::scope::LVScopeId;
use crate::utils::position_to_offset;
use std::sync::Arc;
use tower_lsp::lsp_types::{Position, Range, Url};

/// Context for hover generation (provides access to necessary data).
pub struct HoverContext<'a> {
    pub index: Index<Unlocked>,
    pub uri: &'a Url,
    pub content: &'a str,
    pub document: Option<&'a Arc<parking_lot::RwLock<RubyDocument>>>,
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
    let (name, position, scope_id) = match node {
        HoverNode::LocalVariable {
            name,
            position,
            scope_id,
        } => (name, position, scope_id),
        _ => return None,
    };

    // Try multiple type resolution strategies (excluding Unknown)
    let from_lvar = get_type_from_document_lvar(context, name, *position, *scope_id, false);
    let from_query = from_lvar.or_else(|| get_type_from_type_query(context, name, *position));
    let resolved_type = from_query.or_else(|| get_type_from_document(context, name, *position));

    match resolved_type {
        Some(t) => Some(HoverInfo::text(t.to_string())),
        None => {
            // If no known type found, check if there's an Unknown type stored
            // and display it as "?" for consistency with inlay hints
            let has_unknown =
                get_type_from_document_lvar(context, name, *position, *scope_id, true).is_some();
            if has_unknown {
                Some(HoverInfo::text("?".to_string()))
            } else {
                Some(HoverInfo::text(name.to_string()))
            }
        }
    }
}

/// Get type from document's local variable tracking.
/// If `include_unknown` is false, Unknown types are filtered out to allow fallback strategies.
/// If `include_unknown` is true, Unknown types are returned (for checking if variable exists).
///
/// This function handles Ruby's closure semantics correctly:
/// - Blocks CAN access variables from enclosing scopes (soft boundary)
/// - Methods/Classes/Modules CANNOT access outer local vars (hard boundary)
///
/// We find the enclosing method and only search for variables within that method's scope.
fn get_type_from_document_lvar(
    context: &HoverContext,
    name: &str,
    position: Position,
    scope_id: LVScopeId,
    include_unknown: bool,
) -> Option<RubyType> {
    let doc_arc = context.document?;
    let doc = doc_arc.read();

    // First, try the exact scope_id
    if let Some(result) = find_var_in_scope(&doc, name, position, scope_id, include_unknown) {
        return Some(result);
    }

    // Find the enclosing method's range - this is the hard scope boundary.
    // Variables outside this method are not accessible (Ruby's scoping rules).
    let enclosing_method_range = find_enclosing_method_range(context, position);

    // Search all scopes for the variable, but respect hard scope boundaries:
    // Only consider variables that are within the same method (or at method level if in a block)
    let all_lvars = doc.get_all_lvars();
    for (_other_scope_id, entries) in all_lvars {
        for entry in entries.iter().rev() {
            if let crate::indexer::entry::entry_kind::EntryKind::LocalVariable(data) = &entry.kind {
                if &data.name == name && entry.location.range.start.line <= position.line {
                    // Check if this variable is within the same hard scope boundary
                    let var_line = entry.location.range.start.line;

                    // If we're inside a method, only accept variables also inside that method
                    if let Some(ref method_range) = enclosing_method_range {
                        if var_line < method_range.start.line || var_line > method_range.end.line {
                            // Variable is outside our enclosing method - skip it
                            continue;
                        }
                    }

                    if let Some(assignment) = data
                        .assignments
                        .iter()
                        .filter(|a| a.range.start.line <= position.line)
                        .last()
                    {
                        if include_unknown || assignment.r#type != RubyType::Unknown {
                            return Some(assignment.r#type.clone());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Find the range of the enclosing method for a position.
/// Returns None if not inside a method (e.g., at class/module level or top-level).
fn find_enclosing_method_range(context: &HoverContext, position: Position) -> Option<Range> {
    let index = context.index.lock();
    let entries = index.file_entries(context.uri);

    // Find the smallest method that contains this position
    let mut best_match: Option<Range> = None;

    for entry in &entries {
        if let EntryKind::Method(_) = &entry.kind {
            let range = &entry.location.range;
            if position.line >= range.start.line && position.line <= range.end.line {
                // This method contains the position
                // Prefer the smallest (most specific) method
                if best_match.is_none()
                    || (range.end.line - range.start.line)
                        < (best_match.as_ref().unwrap().end.line
                            - best_match.as_ref().unwrap().start.line)
                {
                    best_match = Some(range.clone());
                }
            }
        }
    }

    best_match
}

/// Helper to find a variable in a specific scope.
fn find_var_in_scope(
    doc: &RubyDocument,
    name: &str,
    position: Position,
    scope_id: LVScopeId,
    include_unknown: bool,
) -> Option<RubyType> {
    let entries = doc.get_local_var_entries(scope_id)?;

    for entry in entries.iter().rev() {
        if let crate::indexer::entry::entry_kind::EntryKind::LocalVariable(data) = &entry.kind {
            if &data.name == name && entry.location.range.start.line <= position.line {
                if let Some(assignment) = data
                    .assignments
                    .iter()
                    .filter(|a| a.range.start.line <= position.line)
                    .last()
                {
                    if include_unknown || assignment.r#type != RubyType::Unknown {
                        return Some(assignment.r#type.clone());
                    }
                }
            }
        }
    }
    None
}

/// Get type from TypeQuery.
fn get_type_from_type_query(
    context: &HoverContext,
    name: &str,
    position: Position,
) -> Option<RubyType> {
    let type_query = TypeQuery::new(
        context.index.clone(),
        context.uri,
        context.content.as_bytes(),
    );
    type_query.get_local_variable_type(name, position)
}

/// Get type from variable tracking in document.
fn get_type_from_document(
    context: &HoverContext,
    name: &str,
    position: Position,
) -> Option<RubyType> {
    let doc_arc = context.document?;
    let doc = doc_arc.read();
    let offset = position_to_offset(context.content, position);
    doc.get_var_type(offset, name).cloned()
}

/// Generate hover info for a constant (class/module).
pub fn generate_constant_hover(node: &HoverNode, context: &HoverContext) -> Option<HoverInfo> {
    let path = match node {
        HoverNode::Constant { path } => path,
        _ => return None,
    };

    let fqn = FullyQualifiedName::namespace(path.to_vec());
    let index = context.index.lock();

    if let Some(entries) = index.get(&fqn) {
        let entry_kind = entries.iter().find_map(|entry| match &entry.kind {
            EntryKind::Class(_) => Some("class"),
            EntryKind::Module(_) => Some("module"),
            _ => None,
        });

        let fqn_str = path
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::");

        let content = match entry_kind {
            Some("class") => format!("class {}", fqn_str),
            Some("module") => format!("module {}", fqn_str),
            _ => fqn_str,
        };

        Some(HoverInfo::text(content))
    } else {
        let fqn_str = path
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::");
        Some(HoverInfo::text(fqn_str))
    }
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

    // Use return type inference
    let mut index = context.index.lock();
    let file_contents: std::collections::HashMap<&Url, &[u8]> =
        std::iter::once((context.uri, context.content.as_bytes())).collect();

    let return_type = crate::inferrer::return_type::infer_method_call(
        &mut index,
        &receiver_type,
        name,
        Some(&file_contents),
    );

    match return_type {
        Some(t) => Some(HoverInfo::ruby_code(t.to_string())),
        None => Some(HoverInfo::ruby_code(format!("def {}", name))),
    }
}

/// Generate hover info for a variable (instance, class, or global).
pub fn generate_variable_hover(node: &HoverNode, context: &HoverContext) -> Option<HoverInfo> {
    let (name, matcher): (&str, fn(&EntryKind) -> Option<(&str, &RubyType)>) = match node {
        HoverNode::InstanceVariable { name } => (name.as_str(), |kind| {
            if let EntryKind::InstanceVariable(data) = kind {
                Some((&data.name, &data.r#type))
            } else {
                None
            }
        }),
        HoverNode::ClassVariable { name } => (name.as_str(), |kind| {
            if let EntryKind::ClassVariable(data) = kind {
                Some((&data.name, &data.r#type))
            } else {
                None
            }
        }),
        HoverNode::GlobalVariable { name } => (name.as_str(), |kind| {
            if let EntryKind::GlobalVariable(data) = kind {
                Some((&data.name, &data.r#type))
            } else {
                None
            }
        }),
        _ => return None,
    };

    let index = context.index.lock();
    let type_str = index.file_entries(context.uri).iter().find_map(|entry| {
        matcher(&entry.kind)
            .filter(|(n, t)| n == &name && *t != &RubyType::Unknown)
            .map(|(_, t)| t.to_string())
    });

    match type_str {
        Some(t) => Some(HoverInfo::text(format!("{}: {}", name, t))),
        None => Some(HoverInfo::text(name.to_string())),
    }
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

fn generate_method_definition_hover(
    method_name: &str,
    position: Position,
    context: &HoverContext,
) -> Option<HoverInfo> {
    let index = context.index.lock();

    // Find method entry at position
    let method_entry = index.file_entries(context.uri).into_iter().find(|entry| {
        if let EntryKind::Method(data) = &entry.kind {
            if data.name.to_string() == method_name {
                let range = &entry.location.range;
                return position.line >= range.start.line && position.line <= range.end.line;
            }
        }
        false
    });

    if let Some(entry) = method_entry {
        if let EntryKind::Method(data) = &entry.kind {
            // Check if we already have a return type
            if let Some(rt) = &data.return_type {
                if *rt != RubyType::Unknown {
                    return Some(HoverInfo::ruby_code(format!(
                        "def {} -> {}",
                        method_name, rt
                    )));
                }
            }

            // Check YARD docs
            if let Some(yard_doc) = &data.yard_doc {
                if let Some(return_type) = yard_doc.format_return_type() {
                    return Some(HoverInfo::ruby_code(format!(
                        "def {} -> {}",
                        method_name, return_type
                    )));
                }
            }

            // Try on-demand inference
            if let Some(pos) = data.return_type_position {
                let owner_fqn = data.owner.clone();
                let entry_id_opt =
                    index
                        .get_entry_ids_for_uri(context.uri)
                        .into_iter()
                        .find(|eid| {
                            if let Some(e) = index.get_entry(*eid) {
                                if let EntryKind::Method(d) = &e.kind {
                                    return d.name.to_string() == method_name
                                        && d.return_type_position == Some(pos);
                                }
                            }
                            false
                        });

                if let Some(entry_id) = entry_id_opt {
                    drop(index); // Release lock before parsing

                    // Parse and infer
                    let parse_result = ruby_prism::parse(context.content.as_bytes());
                    let node = parse_result.node();

                    if let Some(def_node) =
                        crate::utils::ast::find_def_node_at_line(&node, pos.line, context.content)
                    {
                        let mut index = context.index.lock();
                        if let Some(inferred_ty) = infer_return_type_for_node(
                            &mut index,
                            context.content.as_bytes(),
                            &def_node,
                            Some(owner_fqn),
                            None,
                        ) {
                            if inferred_ty != RubyType::Unknown {
                                // Cache in index
                                index.update_method_return_type(entry_id, inferred_ty.clone());

                                return Some(HoverInfo::ruby_code(format!(
                                    "def {} -> {}",
                                    method_name, inferred_ty
                                )));
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback - just show the method name
    Some(HoverInfo::ruby_code(format!("def {}", method_name)))
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
                let index = context.index.lock();
                let is_module = index.get(&fqn).map_or(false, |entries| {
                    entries
                        .iter()
                        .any(|e| matches!(e.kind, EntryKind::Module(_)))
                });
                if is_module {
                    RubyType::Module(fqn)
                } else {
                    RubyType::Class(fqn)
                }
            }
        }
        MethodReceiver::Constant(path) => {
            let fqn = FullyQualifiedName::Constant(path.clone());
            RubyType::ClassReference(fqn)
        }
        MethodReceiver::LocalVariable(name) => {
            // Use TypeQuery for local variable type
            let type_query = TypeQuery::new(
                context.index.clone(),
                context.uri,
                context.content.as_bytes(),
            );

            // Try TypeQuery first
            if let Some(t) = type_query.get_local_variable_type(name, position) {
                return t;
            }

            // Try document lvars
            if let Some(doc_arc) = &context.document {
                let doc = doc_arc.read();
                if let Some(entries) = doc.get_local_var_entries(scope_id) {
                    for entry in entries.iter().rev() {
                        if let EntryKind::LocalVariable(data) = &entry.kind {
                            if &data.name == name
                                && entry.location.range.start.line <= position.line
                            {
                                if let Some(assignment) = data
                                    .assignments
                                    .iter()
                                    .filter(|a| a.range.start.line <= position.line)
                                    .last()
                                {
                                    if assignment.r#type != RubyType::Unknown {
                                        return assignment.r#type.clone();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Try type snapshots
            if let Some(t) = get_type_from_document(context, name, position) {
                return t;
            }

            RubyType::Unknown
        }
        MethodReceiver::InstanceVariable(name) => {
            let index = context.index.lock();
            index
                .file_entries(context.uri)
                .iter()
                .find_map(|entry| {
                    if let EntryKind::InstanceVariable(data) = &entry.kind {
                        if &data.name == name && data.r#type != RubyType::Unknown {
                            return Some(data.r#type.clone());
                        }
                    }
                    None
                })
                .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::ClassVariable(name) => {
            let index = context.index.lock();
            index
                .file_entries(context.uri)
                .iter()
                .find_map(|entry| {
                    if let EntryKind::ClassVariable(data) = &entry.kind {
                        if &data.name == name && data.r#type != RubyType::Unknown {
                            return Some(data.r#type.clone());
                        }
                    }
                    None
                })
                .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::GlobalVariable(name) => {
            let index = context.index.lock();
            index
                .file_entries(context.uri)
                .iter()
                .find_map(|entry| {
                    if let EntryKind::GlobalVariable(data) = &entry.kind {
                        if &data.name == name && data.r#type != RubyType::Unknown {
                            return Some(data.r#type.clone());
                        }
                    }
                    None
                })
                .unwrap_or(RubyType::Unknown)
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

            let mut index = context.index.lock();
            let file_contents: std::collections::HashMap<&Url, &[u8]> =
                std::iter::once((context.uri, context.content.as_bytes())).collect();

            crate::inferrer::return_type::infer_method_call(
                &mut index,
                &inner_type,
                method_name,
                Some(&file_contents),
            )
            .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::Expression => RubyType::Unknown,
    }
}
