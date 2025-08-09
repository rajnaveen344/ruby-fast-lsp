use log::debug;
use tower_lsp::lsp_types::{Location, Position};

use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::ruby_variable::{RubyVariable, RubyVariableType};

/// Find definitions for a Ruby variable
pub fn find_variable_definitions(
    variable: &RubyVariable,
    index: &RubyIndex,
    _ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    let var_name = variable.name().clone();
    let var_type = variable.variable_type();

    match var_type {
        RubyVariableType::Local(scope_stack) => {
            // Handle local variables with scope
            let mut scope_stack = scope_stack.clone();
            while !scope_stack.is_empty() {
                let var_type = RubyVariableType::Local(scope_stack.clone());
                if let Ok(var) = RubyVariable::new(&var_name, var_type) {
                    let fqn = FullyQualifiedName::variable(var.clone());
                    debug!(
                        "Looking for local variable definition with scope: {:?}",
                        fqn
                    );
                    if let Some(entries) = index.definitions.get(&fqn.into()) {
                        // Note: Position filtering would need to be done at the caller level
                        // since we don't have access to the cursor position here
                        found_locations.extend(entries.iter().map(|e| e.location.clone()));
                        if !found_locations.is_empty() {
                            return Some(found_locations);
                        }
                    }
                }

                // If we found a definition in this scope, return it
                if !found_locations.is_empty() {
                    return Some(found_locations);
                }
                scope_stack.pop();
            }
        }
        RubyVariableType::Instance => {
            if let Ok(var) = RubyVariable::new(&var_name, RubyVariableType::Instance) {
                let fqn = FullyQualifiedName::variable(var.clone());
                debug!("Looking for instance variable definition: {:?}", fqn);
                if let Some(entries) = index.definitions.get(&fqn.into()) {
                    found_locations.extend(entries.iter().map(|e| e.location.clone()));
                }
            }
        }
        RubyVariableType::Class => {
            // For class variables, we only need to check the class/module scope
            if let Ok(var) = RubyVariable::new(&var_name, RubyVariableType::Class) {
                let fqn = FullyQualifiedName::variable(var.clone());
                debug!("Looking for class variable definition: {:?}", fqn);
                if let Some(entries) = index.definitions.get(&fqn.into()) {
                    found_locations.extend(entries.iter().map(|e| e.location.clone()));
                }
            }
        }
        RubyVariableType::Global => {
            if let Ok(var) = RubyVariable::new(&var_name, RubyVariableType::Global) {
                let fqn = FullyQualifiedName::variable(var.clone());
                debug!("Looking for global variable definition: {:?}", fqn);
                if let Some(entries) = index.definitions.get(&fqn.into()) {
                    found_locations.extend(entries.iter().map(|e| e.location.clone()));
                }
            }
        }
    }

    if !found_locations.is_empty() {
        Some(found_locations)
    } else {
        None
    }
}

/// Find variable definitions at a specific position
///
/// This function handles position filtering for local variables internally,
/// ensuring that only definitions that appear before the cursor position are returned.
/// For non-local variables, it delegates to the regular find_variable_definitions function.
pub fn find_variable_definitions_at_position(
    variable: &RubyVariable,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
    position: Position,
) -> Option<Vec<Location>> {
    let var_type = variable.variable_type();

    // For local variables, we need position filtering
    if let RubyVariableType::Local(_) = var_type {
        find_local_variable_definitions_with_position_filter(variable, index, position)
    } else {
        // For non-local variables, position doesn't matter
        find_variable_definitions(variable, index, ancestors)
    }
}

/// Find local variable definitions with position filtering
fn find_local_variable_definitions_with_position_filter(
    variable: &RubyVariable,
    index: &RubyIndex,
    position: Position,
) -> Option<Vec<Location>> {
    let RubyVariableType::Local(scope_stack) = variable.variable_type() else {
        return None;
    };

    let mut found_locations = Vec::new();
    let var_name = variable.name().clone();
    let mut scope_stack = scope_stack.clone();

    while !scope_stack.is_empty() {
        let var_type = RubyVariableType::Local(scope_stack.clone());
        if let Ok(var) = RubyVariable::new(&var_name, var_type) {
            let fqn = FullyQualifiedName::variable(var.clone());
            debug!(
                "Looking for local variable definition with scope: {:?}",
                fqn
            );
            if let Some(entries) = index.definitions.get(&fqn.into()) {
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
        }

        // If we found a definition in this scope, return it
        if !found_locations.is_empty() {
            return Some(found_locations);
        }
        scope_stack.pop();
    }

    None
}

/// Find variable definitions with position filtering for local variables
///
/// @deprecated Use find_variable_definitions_at_position instead
pub fn find_variable_definitions_with_position(
    variable: &RubyVariable,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
    position: Position,
) -> Option<Vec<Location>> {
    find_variable_definitions_at_position(variable, index, ancestors, position)
}
