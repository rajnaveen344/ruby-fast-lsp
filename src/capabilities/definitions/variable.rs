use tower_lsp::lsp_types::{Location, Position};

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::scope::LVScopeStack;

/// Find definitions for a local variable using document.lvars (file-local storage)
pub fn find_local_variable_definitions(
    name: &str,
    scope: &LVScopeStack,
    document: &RubyDocument,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let scope_ids: Vec<_> = scope.iter().rev().map(|s| s.scope_id()).collect();

    // First, try exact scope ID match
    for &scope_id in &scope_ids {
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::LocalVariable { name: var_name, .. } = &entry.kind {
                    if var_name == name {
                        return Some(vec![entry.location.clone()]);
                    }
                }
            }
        }
    }

    // Fallback: search all scopes in the document for this variable name
    if let Some(location) = document.find_local_var_by_name(name) {
        return Some(vec![location]);
    }

    None
}

/// Find definitions for an instance variable
pub fn find_instance_variable_definitions(
    name: &str,
    index: &RubyIndex,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let fqn = FullyQualifiedName::instance_variable(name.to_string()).unwrap();
    if let Some(entries) = index.get(&fqn) {
        let locations = entries.iter().map(|e| e.location.clone()).collect();
        Some(locations)
    } else {
        None
    }
}

/// Find definitions for a class variable
pub fn find_class_variable_definitions(
    name: &str,
    index: &RubyIndex,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let fqn = FullyQualifiedName::class_variable(name.to_string()).unwrap();
    if let Some(entries) = index.get(&fqn) {
        let locations = entries.iter().map(|e| e.location.clone()).collect();
        Some(locations)
    } else {
        None
    }
}

/// Find definitions for a global variable
pub fn find_global_variable_definitions(
    name: &str,
    index: &RubyIndex,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let fqn = FullyQualifiedName::global_variable(name.to_string()).unwrap();
    if let Some(entries) = index.get(&fqn) {
        let locations = entries.iter().map(|e| e.location.clone()).collect();
        Some(locations)
    } else {
        None
    }
}

/// Find local variable definitions at a specific position with position filtering
pub fn find_local_variable_definitions_at_position(
    name: &str,
    scope: &LVScopeStack,
    document: &RubyDocument,
    position: Position,
) -> Option<Vec<Location>> {
    let scope_ids: Vec<_> = scope.iter().rev().map(|s| s.scope_id()).collect();

    // First, try exact scope ID match with position filter
    for &scope_id in &scope_ids {
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::LocalVariable { name: var_name, .. } = &entry.kind {
                    if var_name == name && entry.location.range.start < position {
                        return Some(vec![entry.location.clone()]);
                    }
                }
            }
        }
    }

    // Fallback: search all scopes in the document for this variable name
    if let Some(location) = document.find_local_var_by_name(name) {
        if location.range.start < position {
            return Some(vec![location]);
        }
    }

    None
}
