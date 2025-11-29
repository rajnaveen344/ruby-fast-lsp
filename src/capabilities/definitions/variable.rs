use log::debug;
use tower_lsp::lsp_types::{Location, Position};

use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::scope::LVScopeStack;

/// Find definitions for a local variable
pub fn find_local_variable_definitions(
    name: &str,
    scope: &LVScopeStack,
    index: &RubyIndex,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    let mut scope_stack = scope.clone();

    while !scope_stack.is_empty() {
        let fqn =
            FullyQualifiedName::local_variable(name.to_string(), scope_stack.clone()).unwrap();
        debug!(
            "Looking for local variable definition with scope: {:?}",
            fqn
        );
        if let Some(entries) = index.definitions.get(&fqn) {
            found_locations.extend(entries.iter().map(|e| e.location.clone()));
            if !found_locations.is_empty() {
                return Some(found_locations);
            }
        }

        if !found_locations.is_empty() {
            return Some(found_locations);
        }
        scope_stack.pop();
    }

    if !found_locations.is_empty() {
        Some(found_locations)
    } else {
        None
    }
}

/// Find definitions for an instance variable
pub fn find_instance_variable_definitions(
    name: &str,
    index: &RubyIndex,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let fqn = FullyQualifiedName::instance_variable(name.to_string()).unwrap();
    debug!("Looking for instance variable definition: {:?}", fqn);
    if let Some(entries) = index.definitions.get(&fqn) {
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
    debug!("Looking for class variable definition: {:?}", fqn);
    if let Some(entries) = index.definitions.get(&fqn) {
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
    debug!("Looking for global variable definition: {:?}", fqn);
    if let Some(entries) = index.definitions.get(&fqn) {
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
    index: &RubyIndex,
    position: Position,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    let mut scope_stack = scope.clone();

    while !scope_stack.is_empty() {
        let fqn =
            FullyQualifiedName::local_variable(name.to_string(), scope_stack.clone()).unwrap();
        debug!(
            "Looking for local variable definition with scope: {:?}",
            fqn
        );
        if let Some(entries) = index.definitions.get(&fqn) {
            // Filter entries that are before the cursor position
            let valid_entries: Vec<_> = entries
                .iter()
                .filter(|e| e.location.range.start < position)
                .map(|e| e.location.clone())
                .collect();

            // Add all valid definitions to the results
            if !valid_entries.is_empty() {
                found_locations.extend(valid_entries);
                return Some(found_locations);
            }
        }

        if !found_locations.is_empty() {
            return Some(found_locations);
        }
        scope_stack.pop();
    }

    None
}
