//! Method Query - Method resolution helpers
//!
//! Consolidates method resolution logic for use by hover, completion, etc.
//! Contains both simple helpers and complex type-aware definition search.

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::analyzer_prism::MethodReceiver;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
use crate::indexer::index::RubyIndex;
use crate::inferrer::method::resolver::MethodResolver;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::utils::position_to_offset;
use log::debug;
use std::collections::HashSet;
use tower_lsp::lsp_types::{Location, Position, Url};

use super::IndexQuery;

/// Information about a resolved method.
#[derive(Debug, Clone)]
pub struct MethodInfo {
    /// The fully qualified name of the method.
    pub fqn: FullyQualifiedName,
    /// The return type if known.
    pub return_type: Option<RubyType>,
    /// Whether this is a class method.
    pub is_class_method: bool,
    /// YARD documentation if available.
    pub documentation: Option<String>,
}

impl IndexQuery {
    /// Find definitions for a Ruby method with type-aware filtering.
    ///
    /// Uses type snapshots to filter results based on receiver type when available.
    pub(super) fn find_method_definitions(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        ancestors: &[RubyConstant],
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        match receiver {
            MethodReceiver::Constant(path) => {
                self.handle_constant_receiver(&Some(path.clone()), method, ancestors)
            }
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                self.find_method_without_receiver(method, ancestors)
            }
            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                // Try to get the receiver's type using type snapshots
                let line = content.lines().nth(position.line as usize).unwrap_or("");
                let before_cursor = &line[..std::cmp::min(position.character as usize, line.len())];
                let receiver_offset = if let Some(var_pos) = before_cursor.rfind(name) {
                    position_to_offset(
                        content,
                        Position {
                            line: position.line,
                            character: var_pos as u32,
                        },
                    )
                } else {
                    position_to_offset(content, position)
                };

                // Try variable type from document
                if let Some(doc_arc) = &self.doc {
                    let doc = doc_arc.read();
                    if let Some(receiver_type) = doc.get_var_type(receiver_offset, name) {
                        debug!("Found receiver type for '{}': {:?}", name, receiver_type);
                        return self.search_by_name_filtered(method, receiver_type);
                    }
                }

                // Fallback: Check for constructor assignment pattern
                if let Some(receiver_type) =
                    self.infer_type_from_constructor_assignment(content, name)
                {
                    debug!("Found constructor type for '{}': {:?}", name, receiver_type);
                    return self.search_by_name_filtered(method, &receiver_type);
                }
                // Final fallback
                self.search_by_name(method)
            }
            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            } => {
                let receiver_type = self.resolve_method_call_type(
                    inner_receiver,
                    method_name,
                    uri,
                    position,
                    content,
                );
                if let Some(ty) = receiver_type {
                    debug!(
                        "Found method call receiver type for '{}.{}': {:?}",
                        inner_receiver_to_string(inner_receiver),
                        method_name,
                        ty
                    );
                    return self.search_by_name_filtered(method, &ty);
                }
                self.search_by_name(method)
            }
            MethodReceiver::Expression => self.search_by_name(method),
        }
    }

    /// Resolve the type of a method call receiver by looking up the method's return type
    pub(super) fn resolve_method_call_type(
        &self,
        inner_receiver: &MethodReceiver,
        method_name: &str,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<RubyType> {
        let inner_type = match inner_receiver {
            MethodReceiver::None | MethodReceiver::SelfReceiver => return None,
            MethodReceiver::Constant(path) => {
                RubyType::ClassReference(FullyQualifiedName::Constant(path.clone()))
            }
            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                let offset = position_to_offset(content, position);

                // Try variable type from document
                if let Some(doc_arc) = &self.doc {
                    let doc = doc_arc.read();
                    if let Some(ty) = doc.get_var_type(offset, name) {
                        ty.clone()
                    } else if let Some(ty) =
                        self.infer_type_from_constructor_assignment(content, name)
                    {
                        ty
                    } else {
                        return None;
                    }
                } else if let Some(ty) = self.infer_type_from_constructor_assignment(content, name)
                {
                    ty
                } else {
                    return None;
                }
            }
            MethodReceiver::MethodCall {
                inner_receiver: nested_receiver,
                method_name: nested_method,
            } => self.resolve_method_call_type(
                nested_receiver,
                nested_method,
                uri,
                position,
                content,
            )?,
            MethodReceiver::Expression => return None,
        };

        let index = self.index.lock();
        MethodResolver::resolve_method_return_type(&index, &inner_type, method_name)
    }
}

// Private helpers
impl IndexQuery {
    // --- Search Helpers ---

    fn handle_constant_receiver(
        &self,
        receiver: &Option<Vec<RubyConstant>>,
        method: &RubyMethod,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        if let Some(receiver_ns) = receiver {
            let index = self.index.lock();
            let current_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
            if let Some(resolved_fqn) =
                resolve_constant_fqn_from_parts(&index, receiver_ns, false, &current_fqn)
            {
                drop(index);
                if let FullyQualifiedName::Constant(resolved_ns) = resolved_fqn {
                    self.find_method_with_receiver(&resolved_ns, method)
                } else {
                    None
                }
            } else {
                drop(index);
                self.find_method_with_receiver(receiver_ns, method)
            }
        } else {
            self.find_method_without_receiver(method, ancestors)
        }
    }

    fn find_method_with_receiver(
        &self,
        ns: &[RubyConstant],
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let receiver_fqn = FullyQualifiedName::Constant(ns.to_vec());
        if is_constant_receiver(method) {
            self.search_direct_references(&receiver_fqn, method)
        } else {
            self.search_by_name(method)
        }
    }

    fn find_method_without_receiver(
        &self,
        method: &RubyMethod,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let receiver_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
        let mut visited = HashSet::new();
        let method_kind = method.get_kind();

        if let Some(locations) = self.search_in_ancestor_chain_with_visited(
            &receiver_fqn,
            method,
            method_kind,
            &mut visited,
        ) {
            return Some(locations);
        }

        if let Some(including_classes) = self.search_in_sibling_modules_with_visited(
            &receiver_fqn,
            method,
            method_kind,
            &mut visited,
        ) {
            return Some(including_classes);
        }

        None
    }

    fn search_by_name_filtered(
        &self,
        method: &RubyMethod,
        receiver_type: &RubyType,
    ) -> Option<Vec<Location>> {
        let type_names = get_type_names(receiver_type);
        if type_names.is_empty() {
            return self.search_by_name(method);
        }

        let index = self.index.lock();
        let mut filtered_locations = Vec::new();

        if let Some(entries) = index.get_methods_by_name(method) {
            for entry in entries.iter() {
                let fqn = match index.get_fqn(entry.fqn_id) {
                    Some(f) => f,
                    None => continue,
                };
                let method_class = fqn.namespace_parts();
                if !method_class.is_empty() {
                    let class_name = method_class
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join("::");

                    if type_names.iter().any(|t| *t == class_name) {
                        if let Some(loc) = index.to_lsp_location(&entry.location) {
                            filtered_locations.push(loc);
                        }
                    }
                }
            }
        }

        if filtered_locations.is_empty() {
            drop(index);
            self.search_by_name(method)
        } else {
            Some(filtered_locations)
        }
    }

    fn search_by_name(&self, method: &RubyMethod) -> Option<Vec<Location>> {
        let index = self.index.lock();
        index.get_methods_by_name(method).and_then(|entries| {
            let locations: Vec<Location> = entries
                .iter()
                .filter_map(|entry| index.to_lsp_location(&entry.location))
                .collect();
            if locations.is_empty() {
                None
            } else {
                Some(locations)
            }
        })
    }

    fn infer_type_from_constructor_assignment(
        &self,
        content: &str,
        var_name: &str,
    ) -> Option<RubyType> {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(var_name) {
                let next_char = rest.chars().next();
                if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
                    continue;
                }
                let rest = rest.trim();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rhs = rest.trim();
                    if let Some(new_pos) = rhs.find(".new") {
                        let class_part = rhs[..new_pos].trim();
                        // Determine if it's a constant
                        if !class_part
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false)
                        {
                            continue;
                        }

                        let parts: Vec<_> = class_part
                            .split("::")
                            .filter_map(|s| RubyConstant::new(s.trim()).ok())
                            .collect();
                        if parts.is_empty() {
                            continue;
                        }

                        let class_fqn = FullyQualifiedName::Constant(parts);
                        let mut current_type = RubyType::Class(class_fqn);

                        // Check method chain
                        let after_new = &rhs[new_pos + 4..];
                        let after_new = if after_new.starts_with('(') {
                            if let Some(close_paren) = after_new.find(')') {
                                &after_new[close_paren + 1..]
                            } else {
                                after_new
                            }
                        } else {
                            after_new
                        };

                        let index = self.index.lock();
                        for method_call in after_new.split('.') {
                            let method_name = method_call
                                .split(|c: char| c == '(' || c.is_whitespace())
                                .next()
                                .unwrap_or("")
                                .trim();

                            if method_name.is_empty() {
                                continue;
                            }

                            if let Some(return_type) = MethodResolver::resolve_method_return_type(
                                &index,
                                &current_type,
                                method_name,
                            ) {
                                current_type = return_type;
                            } else {
                                break;
                            }
                        }
                        return Some(current_type);
                    }
                }
            }
        }
        None
    }

    fn search_direct_references(
        &self,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let mut found_locations = Vec::new();
        let mut visited = HashSet::new();

        let kinds_to_check = if method.get_kind() == MethodKind::Unknown {
            vec![MethodKind::Instance, MethodKind::Class]
        } else {
            vec![method.get_kind()]
        };

        for kind in kinds_to_check {
            if let Some(locations) =
                self.search_in_ancestor_chain_with_visited(receiver_fqn, method, kind, &mut visited)
            {
                found_locations.extend(locations);
            }
        }

        if found_locations.is_empty() {
            None
        } else {
            Some(deduplicate_locations(found_locations))
        }
    }

    fn search_in_ancestor_chain_with_visited(
        &self,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: MethodKind,
        visited: &mut HashSet<FullyQualifiedName>,
    ) -> Option<Vec<Location>> {
        if visited.contains(receiver_fqn) {
            return None;
        }
        visited.insert(receiver_fqn.clone());

        let index = self.index.lock();
        let found_locations = if Self::is_class_context_static(&index, receiver_fqn) {
            Self::search_method_in_class_hierarchy_static(&index, receiver_fqn, method, kind)
        } else {
            Self::search_method_in_including_classes_static(&index, receiver_fqn, method)
        };

        if found_locations.is_empty() {
            None
        } else {
            Some(found_locations)
        }
    }

    fn search_method_in_class_hierarchy_static(
        index: &RubyIndex,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: MethodKind,
    ) -> Vec<Location> {
        let mut found_locations = Vec::new();
        let is_class_method = kind == MethodKind::Class;

        let mut modules_to_search = HashSet::new();
        modules_to_search.insert(receiver_fqn.clone());

        let ancestor_chain = index.get_ancestor_chain(receiver_fqn, is_class_method);

        for ancestor_fqn in &ancestor_chain {
            modules_to_search.insert(ancestor_fqn.clone());
            let included_modules = Self::get_included_modules_static(index, ancestor_fqn);
            for module_fqn in included_modules {
                Self::collect_all_searchable_modules_static(
                    index,
                    &module_fqn,
                    &mut modules_to_search,
                );
            }
        }

        for module_fqn in &modules_to_search {
            let method_fqn =
                FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
            if let Some(entries) = index.get(&method_fqn) {
                found_locations.extend(
                    entries
                        .iter()
                        .filter_map(|e| index.to_lsp_location(&e.location)),
                );
            }
        }

        deduplicate_locations(found_locations)
    }

    fn search_method_in_including_classes_static(
        index: &RubyIndex,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<Location> {
        let mut found_locations = Vec::new();
        let mut modules_to_search = HashSet::new();

        modules_to_search.insert(receiver_fqn.clone());

        let including_classes = index.get_including_classes(receiver_fqn);

        for class_fqn in including_classes {
            Self::collect_all_searchable_modules_static(index, &class_fqn, &mut modules_to_search);
            let included_modules = Self::get_included_modules_static(index, &class_fqn);
            for module_fqn in included_modules {
                Self::collect_all_searchable_modules_static(
                    index,
                    &module_fqn,
                    &mut modules_to_search,
                );
            }
        }

        for module_fqn in &modules_to_search {
            let method_fqn =
                FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
            if let Some(entries) = index.get(&method_fqn) {
                found_locations.extend(
                    entries
                        .iter()
                        .filter_map(|e| index.to_lsp_location(&e.location)),
                );
            }
        }
        deduplicate_locations(found_locations)
    }

    fn get_included_modules_static(
        index: &RubyIndex,
        class_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        let mut included_modules = Vec::new();
        let mut seen_modules = HashSet::<FullyQualifiedName>::new();

        let ancestor_chain = index.get_ancestor_chain(class_fqn, false);

        for ancestor_fqn in &ancestor_chain {
            if let Some(entries) = index.get(ancestor_fqn) {
                for entry in entries.iter() {
                    Self::process_entry_mixins_static(
                        index,
                        &entry.kind,
                        ancestor_fqn,
                        &mut included_modules,
                        &mut seen_modules,
                    );
                }
            }
        }
        included_modules
    }

    fn process_entry_mixins_static(
        index: &RubyIndex,
        entry_kind: &EntryKind,
        ancestor_fqn: &FullyQualifiedName,
        included_modules: &mut Vec<FullyQualifiedName>,
        seen_modules: &mut HashSet<FullyQualifiedName>,
    ) {
        let (includes, extends, prepends) = match entry_kind {
            EntryKind::Class(data) => (&data.includes, &data.extends, &data.prepends),
            EntryKind::Module(data) => (&data.includes, &data.extends, &data.prepends),
            _ => return,
        };

        Self::process_mixins_static(
            index,
            prepends,
            ancestor_fqn,
            included_modules,
            seen_modules,
            true,
        );
        Self::process_mixins_static(
            index,
            includes,
            ancestor_fqn,
            included_modules,
            seen_modules,
            false,
        );
        Self::process_mixins_static(
            index,
            extends,
            ancestor_fqn,
            included_modules,
            seen_modules,
            false,
        );
    }

    fn process_mixins_static(
        index: &RubyIndex,
        mixins: &[crate::indexer::entry::MixinRef],
        ancestor_fqn: &FullyQualifiedName,
        included_modules: &mut Vec<FullyQualifiedName>,
        seen_modules: &mut HashSet<FullyQualifiedName>,
        reverse_order: bool,
    ) {
        let iter: Box<dyn Iterator<Item = _>> = if reverse_order {
            Box::new(mixins.iter().rev())
        } else {
            Box::new(mixins.iter())
        };

        for mixin_ref in iter {
            if let Some(resolved_fqn) = resolve_constant_fqn_from_parts(
                index,
                &mixin_ref.parts,
                mixin_ref.absolute,
                ancestor_fqn,
            ) {
                if seen_modules.insert(resolved_fqn.clone()) {
                    included_modules.push(resolved_fqn);
                }
            }
        }
    }

    fn is_class_context_static(index: &RubyIndex, fqn: &FullyQualifiedName) -> bool {
        if let Some(entries) = index.get(fqn) {
            for entry in entries {
                match &entry.kind {
                    EntryKind::Class(_) => return true,
                    EntryKind::Module(_) => return false,
                    _ => continue,
                }
            }
        }
        true
    }

    fn collect_all_searchable_modules_static(
        index: &RubyIndex,
        fqn: &FullyQualifiedName,
        modules_to_search: &mut HashSet<FullyQualifiedName>,
    ) {
        if modules_to_search.contains(fqn) {
            return;
        }
        modules_to_search.insert(fqn.clone());

        let ancestor_chain = index.get_ancestor_chain(fqn, false);
        for ancestor_fqn in &ancestor_chain {
            if !modules_to_search.contains(ancestor_fqn) {
                modules_to_search.insert(ancestor_fqn.clone());
            }
        }

        let included_modules = Self::get_included_modules_static(index, fqn);
        for module_fqn in included_modules {
            Self::collect_all_searchable_modules_static(index, &module_fqn, modules_to_search);
        }
    }

    fn search_in_sibling_modules_with_visited(
        &self,
        class_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: MethodKind,
        visited: &mut HashSet<FullyQualifiedName>,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let included_modules = Self::get_included_modules_static(&index, class_fqn);
        drop(index);

        let mut found_locations = Vec::new();
        for module_fqn in included_modules {
            if let Some(locations) =
                self.search_in_ancestor_chain_with_visited(&module_fqn, method, kind, visited)
            {
                found_locations.extend(locations);
            }
        }

        if found_locations.is_empty() {
            None
        } else {
            Some(found_locations)
        }
    }
}

// --- Utils ---

fn inner_receiver_to_string(receiver: &MethodReceiver) -> String {
    match receiver {
        MethodReceiver::None => "".to_string(),
        MethodReceiver::SelfReceiver => "self".to_string(),
        MethodReceiver::Constant(path) => path
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        MethodReceiver::LocalVariable(name)
        | MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => name.clone(),
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => format!(
            "{}.{}",
            inner_receiver_to_string(inner_receiver),
            method_name
        ),
        MethodReceiver::Expression => "<expr>".to_string(),
    }
}

fn is_constant_receiver(method: &RubyMethod) -> bool {
    method.get_kind() == MethodKind::Class || method.get_kind() == MethodKind::Unknown
}

fn deduplicate_locations(locations: Vec<Location>) -> Vec<Location> {
    let mut unique_locations = Vec::new();
    for location in locations {
        if !unique_locations.iter().any(|existing: &Location| {
            existing.uri == location.uri && existing.range == location.range
        }) {
            unique_locations.push(location);
        }
    }
    unique_locations
}

fn get_type_names(ty: &RubyType) -> Vec<String> {
    match ty {
        RubyType::Class(fqn) => vec![fqn.to_string()],
        RubyType::ClassReference(fqn) => vec![fqn.to_string()],
        RubyType::Module(fqn) => vec![fqn.to_string()],
        RubyType::ModuleReference(fqn) => vec![fqn.to_string()],
        RubyType::Array(_) => vec!["Array".to_string()],
        RubyType::Hash(_, _) => vec!["Hash".to_string()],
        RubyType::Union(types) => types.iter().flat_map(get_type_names).collect(),
        RubyType::Unknown => vec![],
    }
}
