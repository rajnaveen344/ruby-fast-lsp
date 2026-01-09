//! Hover Query - Unified hover information retrieval
//!
//! Provides the unified `get_hover_at_position` API that handles all identifier types.
//! This consolidates hover logic previously spread across capabilities/hover.rs.

use crate::analyzer_prism::{Identifier, IdentifierType, MethodReceiver, RubyPrismAnalyzer};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::return_type::infer_return_type_for_node;
use crate::inferrer::TypeNarrowingEngine;
use crate::query::TypeQuery;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::scope::LVScopeId;
use crate::utils::position_to_offset;
use tower_lsp::lsp_types::{Position, Range, Url};

use super::IndexQuery;

/// Hover information for a symbol.
#[derive(Debug, Clone)]
pub struct HoverInfo {
    /// The markdown content to display.
    pub content: String,
    /// The range of the hovered symbol.
    pub range: Option<Range>,
    /// The inferred type (if applicable).
    pub ruby_type: Option<RubyType>,
}

impl HoverInfo {
    /// Create hover info with just text content.
    pub fn text(content: String) -> Self {
        Self {
            content,
            range: None,
            ruby_type: None,
        }
    }

    /// Create hover info formatted as Ruby code block.
    pub fn ruby_code(content: String) -> Self {
        Self {
            content: format!("```ruby\n{}\n```", content),
            range: None,
            ruby_type: None,
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

impl IndexQuery {
    /// Get hover info for the symbol at position.
    ///
    /// This is the unified entry point for hover requests. It handles:
    /// - Local variables (with type inference from TypeQuery, document lvars, type narrowing)
    /// - Instance/class/global variables
    /// - Constants (classes, modules)
    /// - Methods (with receiver type resolution and return type inference)
    /// - YARD type references
    pub fn get_hover_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
        type_narrowing: Option<&TypeNarrowingEngine>,
    ) -> Option<HoverInfo> {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, identifier_type, namespace, scope_id) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        match &identifier {
            Identifier::RubyLocalVariable { name, .. } => self.get_local_variable_hover(
                uri,
                position,
                content,
                name,
                scope_id,
                type_narrowing,
            ),
            Identifier::RubyConstant { iden, .. } => self.get_constant_hover(iden),
            Identifier::RubyMethod {
                iden,
                receiver,
                namespace: method_ns,
            } => {
                let method_name = iden.to_string();
                let is_method_definition = identifier_type == Some(IdentifierType::MethodDef);

                // Use method_ns if available, otherwise fall back to namespace from analyzer
                let ns = if method_ns.is_empty() {
                    &namespace
                } else {
                    method_ns
                };

                self.get_method_hover(
                    uri,
                    position,
                    content,
                    &method_name,
                    receiver,
                    ns,
                    scope_id,
                    is_method_definition,
                    type_narrowing,
                )
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                self.get_instance_variable_hover(uri, name)
            }
            Identifier::RubyClassVariable { name, .. } => self.get_class_variable_hover(uri, name),
            Identifier::RubyGlobalVariable { name, .. } => {
                self.get_global_variable_hover(uri, name)
            }
            Identifier::YardType { type_name, .. } => Some(HoverInfo::text(type_name.clone())),
        }
    }
}

// =============================================================================
// Private helpers - Local Variables
// =============================================================================

impl IndexQuery {
    fn get_local_variable_hover(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
        name: &str,
        scope_id: LVScopeId,
        type_narrowing: Option<&TypeNarrowingEngine>,
    ) -> Option<HoverInfo> {
        // 1. Check document lvars first
        let from_lvar = self.doc.as_ref().and_then(|doc_arc| {
            let doc = doc_arc.read();
            doc.get_local_var_entries(scope_id).and_then(|entries| {
                entries
                    .iter()
                    .filter(|entry| {
                        if let EntryKind::LocalVariable(data) = &entry.kind {
                            &data.name == name && entry.location.range.start.line <= position.line
                        } else {
                            false
                        }
                    })
                    .last()
                    .and_then(|entry| {
                        if let EntryKind::LocalVariable(data) = &entry.kind {
                            data.assignments
                                .iter()
                                .filter(|a| a.range.start.line <= position.line)
                                .last()
                                .map(|a| &a.r#type)
                                .filter(|ty| **ty != RubyType::Unknown)
                                .cloned()
                        } else {
                            None
                        }
                    })
            })
        });

        // 2. Try TypeQuery (method params, assignment inference)
        let from_query = from_lvar.clone().or_else(|| {
            let type_query = TypeQuery::new(self.index.clone(), uri, content.as_bytes());
            type_query.get_local_variable_type(name, position)
        });

        // 3. Try type narrowing engine
        let resolved_type = from_query.or_else(|| {
            type_narrowing.and_then(|narrowing| {
                let offset = position_to_offset(content, position);
                narrowing.get_narrowed_type(uri, offset, Some(content))
            })
        });

        // Update document if we found a better type
        if let (Some(ref t), Some(doc_arc)) = (&resolved_type, &self.doc) {
            if *t != RubyType::Unknown && from_lvar.is_none() {
                let range_opt = {
                    let doc = doc_arc.read();
                    doc.get_local_var_entries(scope_id).and_then(|entries| {
                        entries
                            .iter()
                            .find(|entry| {
                                if let EntryKind::LocalVariable(data) = &entry.kind {
                                    if &data.name == name {
                                        let r = &entry.location.range;
                                        return position.line >= r.start.line
                                            && position.line <= r.end.line;
                                    }
                                }
                                false
                            })
                            .map(|e| e.location.range)
                    })
                };

                if let Some(range) = range_opt {
                    let mut doc = doc_arc.write();
                    doc.update_local_var_type(scope_id, name, range, t.clone());
                }
            }
        }

        match resolved_type {
            Some(t) => Some(HoverInfo::text(t.to_string())),
            None => Some(HoverInfo::text(name.to_string())),
        }
    }
}

// =============================================================================
// Private helpers - Constants
// =============================================================================

impl IndexQuery {
    fn get_constant_hover(&self, constant_path: &[RubyConstant]) -> Option<HoverInfo> {
        let fqn = FullyQualifiedName::namespace(constant_path.to_vec());
        let index = self.index.lock();

        if let Some(entries) = index.get(&fqn) {
            let entry_kind = entries.iter().find_map(|entry| match &entry.kind {
                EntryKind::Class(_) => Some("class"),
                EntryKind::Module(_) => Some("module"),
                _ => None,
            });

            let fqn_str = constant_path
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
            let fqn_str = constant_path
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("::");
            Some(HoverInfo::text(fqn_str))
        }
    }
}

// =============================================================================
// Private helpers - Methods
// =============================================================================

impl IndexQuery {
    fn get_method_hover(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
        method_name: &str,
        receiver: &MethodReceiver,
        namespace: &[RubyConstant],
        scope_id: LVScopeId,
        is_method_definition: bool,
        type_narrowing: Option<&TypeNarrowingEngine>,
    ) -> Option<HoverInfo> {
        // Special handling for .new - return the class instance type
        if method_name == "new" && !is_method_definition {
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
        if is_method_definition {
            return self.get_method_definition_hover(uri, content, position, method_name);
        }

        // For method calls, resolve receiver type and infer return type
        let receiver_type = self.resolve_receiver_type(
            uri,
            position,
            content,
            receiver,
            namespace,
            scope_id,
            type_narrowing,
        );

        // Use return type inference
        let mut index = self.index.lock();
        let file_contents: std::collections::HashMap<&Url, &[u8]> =
            std::iter::once((uri, content.as_bytes())).collect();

        let return_type = crate::inferrer::return_type::infer_method_call(
            &mut index,
            &receiver_type,
            method_name,
            Some(&file_contents),
        );

        match return_type {
            Some(t) => Some(HoverInfo::ruby_code(t.to_string())),
            None => Some(HoverInfo::ruby_code(format!("def {}", method_name))),
        }
    }

    fn resolve_receiver_type(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
        receiver: &MethodReceiver,
        namespace: &[RubyConstant],
        scope_id: LVScopeId,
        type_narrowing: Option<&TypeNarrowingEngine>,
    ) -> RubyType {
        match receiver {
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                if namespace.is_empty() {
                    RubyType::class("Object")
                } else {
                    let fqn = FullyQualifiedName::from(namespace.to_vec());
                    let index = self.index.lock();
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
                let type_query = TypeQuery::new(self.index.clone(), uri, content.as_bytes());

                // Try TypeQuery first
                if let Some(t) = type_query.get_local_variable_type(name, position) {
                    return t;
                }

                // Try document lvars
                if let Some(doc_arc) = &self.doc {
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

                // Try type narrowing
                if let Some(narrowing) = type_narrowing {
                    let offset = position_to_offset(content, position);
                    if let Some(t) = narrowing.get_narrowed_type(uri, offset, Some(content)) {
                        return t;
                    }
                }

                RubyType::Unknown
            }
            MethodReceiver::InstanceVariable(name) => {
                let index = self.index.lock();
                index
                    .file_entries(uri)
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
                let index = self.index.lock();
                index
                    .file_entries(uri)
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
                let index = self.index.lock();
                index
                    .file_entries(uri)
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

                let inner_type = self.resolve_receiver_type(
                    uri,
                    position,
                    content,
                    inner_receiver,
                    namespace,
                    scope_id,
                    type_narrowing,
                );

                if inner_type == RubyType::Unknown {
                    return RubyType::Unknown;
                }

                let mut index = self.index.lock();
                let file_contents: std::collections::HashMap<&Url, &[u8]> =
                    std::iter::once((uri, content.as_bytes())).collect();

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

    fn get_method_definition_hover(
        &self,
        uri: &Url,
        content: &str,
        position: Position,
        method_name: &str,
    ) -> Option<HoverInfo> {
        let index = self.index.lock();

        // Find method entry at position
        let method_entry = index.file_entries(uri).into_iter().find(|entry| {
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
                    let entry_id_opt = index.get_entry_ids_for_uri(uri).into_iter().find(|eid| {
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
                        let parse_result = ruby_prism::parse(content.as_bytes());
                        let node = parse_result.node();

                        if let Some(def_node) = find_def_node_at_line(&node, pos.line, content) {
                            let mut index = self.index.lock();
                            if let Some(inferred_ty) = infer_return_type_for_node(
                                &mut index,
                                content.as_bytes(),
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
}

// =============================================================================
// Private helpers - Variables
// =============================================================================

impl IndexQuery {
    fn get_instance_variable_hover(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
        let index = self.index.lock();
        let type_str = index.file_entries(uri).iter().find_map(|entry| {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    return Some(data.r#type.to_string());
                }
            }
            None
        });

        match type_str {
            Some(t) => Some(HoverInfo::text(format!("{}: {}", name, t))),
            None => Some(HoverInfo::text(name.to_string())),
        }
    }

    fn get_class_variable_hover(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
        let index = self.index.lock();
        let type_str = index.file_entries(uri).iter().find_map(|entry| {
            if let EntryKind::ClassVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    return Some(data.r#type.to_string());
                }
            }
            None
        });

        match type_str {
            Some(t) => Some(HoverInfo::text(format!("{}: {}", name, t))),
            None => Some(HoverInfo::text(name.to_string())),
        }
    }

    fn get_global_variable_hover(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
        let index = self.index.lock();
        let type_str = index.file_entries(uri).iter().find_map(|entry| {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    return Some(data.r#type.to_string());
                }
            }
            None
        });

        match type_str {
            Some(t) => Some(HoverInfo::text(format!("{}: {}", name, t))),
            None => Some(HoverInfo::text(name.to_string())),
        }
    }
}

// =============================================================================
// AST Helpers
// =============================================================================

/// Find a DefNode at the given line in the AST.
fn find_def_node_at_line<'a>(
    node: &ruby_prism::Node<'a>,
    target_line: u32,
    content: &str,
) -> Option<ruby_prism::DefNode<'a>> {
    // Try to match DefNode
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        let line = content.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    None
}
