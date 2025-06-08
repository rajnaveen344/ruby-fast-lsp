use log::{debug, info};
use lsp_types::{Location, Position, Url};
use std::time::Instant;

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::{entry_kind::EntryKind, MethodOrigin};
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_variable::{RubyVariable, RubyVariableType};
use crate::types::scope_kind::{LVScopeDepth, LVScopeKind};

/// Find the definition(s) of a symbol at the given position
///
/// Returns a vector of locations if definitions are found, or None if no definitions are found.
/// Multiple definitions can be returned when a symbol is defined in multiple places.
pub async fn find_definition_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
    content: &str,
) -> Option<Vec<Location>> {
    // Use the analyzer to find the identifier at the position
    let analyzer = RubyPrismAnalyzer::new(content.to_string());

    let start_time = Instant::now();
    let (identifier, ancestors) = analyzer.get_identifier(position);
    let identifier_time = start_time.elapsed();
    info!("Performance: get_identifier took {:?}", identifier_time);

    // Extract the identifier if available
    if let None = identifier {
        info!("No identifier found at position {:?}", position);
        return None;
    }

    info!(
        "Looking for definition of: {:?}->{}",
        identifier.clone().unwrap(),
        FullyQualifiedName::from(ancestors.clone()),
    );

    // Get the index and search for the definition
    let index = server.index.lock().unwrap();
    let identifier = identifier.unwrap();
    let mut found_locations = Vec::new();

    // If not found directly, try based on the identifier type
    match identifier.clone() {
        Identifier::RubyConstant(ns) => {
            // Start with the current namespace and ancestors
            let mut search_namespaces = ancestors.clone();

            // Search through ancestor namespaces
            while !search_namespaces.is_empty() {
                // For each ancestor, try to find the namespace
                let mut combined_ns = search_namespaces.clone();
                combined_ns.extend(ns.iter().cloned());

                let search_fqn = Identifier::RubyConstant(combined_ns);

                if let Some(entries) = index.definitions.get(&search_fqn.clone().into()) {
                    if !entries.is_empty() {
                        // Add all locations to our result
                        for entry in entries {
                            found_locations.push(entry.location.clone());
                        }
                        return Some(found_locations);
                    }
                }

                // Pop the last namespace and try again
                search_namespaces.pop();
            }

            // Try at the top level
            let top_level_fqn = Identifier::RubyConstant(ns.clone());
            if let Some(entries) = index.definitions.get(&top_level_fqn.clone().into()) {
                if !entries.is_empty() {
                    for entry in entries {
                        found_locations.push(entry.location.clone());
                    }
                    return Some(found_locations);
                }
            }
        }
        Identifier::RubyMethod(ns, method) => {
            // Start with the exact identifier
            let iden = Identifier::RubyMethod(ns, method.clone());
            info!("Searching for method with identifier: {:?}", iden.clone());

            // First try to find the method with the exact namespace
            if let Some(entries) = index.methods_by_name.get(&method) {
                if !entries.is_empty() {
                    // Include all methods with Direct origin
                    for entry in entries {
                        if let EntryKind::Method { origin, .. } = &entry.kind {
                            info!("Checking method entry: origin={:?}", origin);
                            if matches!(origin, MethodOrigin::Direct) {
                                info!("Adding location: {:?}", entry.location);
                                found_locations.push(entry.location.clone());
                            }
                        }
                    }

                    if !found_locations.is_empty() {
                        return Some(found_locations);
                    }
                }
            }
        }
        Identifier::RubyVariable(method, variable) => {
            let mut found_locations = Vec::new();

            let scope_stack = variable.variable_type();

            let mut scope_stack = match scope_stack {
                RubyVariableType::Local(_, _, scope_stack) => scope_stack.clone(),
                _ => Vec::new(),
            };

            while !scope_stack.is_empty() {
                let var_scope = scope_stack.last().unwrap();
                let var_type = RubyVariableType::Local(
                    scope_stack.len() as LVScopeDepth,
                    var_scope.clone(),
                    scope_stack.clone(),
                );
                if let Ok(var) = RubyVariable::new(variable.name(), var_type) {
                    let fqn = FullyQualifiedName::variable(
                        uri.clone(),
                        ancestors.clone(),
                        method.clone(),
                        var,
                    );
                    info!("Looking for variable definition in parent scope: {:?}", fqn);
                    if let Some(entries) = index.definitions.get(&fqn.into()) {
                        found_locations.extend(entries.iter().map(|e| e.location.clone()));
                    }
                }

                if !found_locations.is_empty() {
                    return Some(found_locations);
                }

                scope_stack.pop();
            }

            // Finally check top-level scope
            let var_type = RubyVariableType::Local(0, LVScopeKind::TopLevel, Vec::new());
            if let Ok(var) = RubyVariable::new(variable.name(), var_type) {
                let fqn = FullyQualifiedName::variable(
                    uri.clone(),
                    ancestors.clone(),
                    method.clone(),
                    var,
                );
                info!("Looking for variable definition in top level: {:?}", fqn);
                if let Some(entries) = index.definitions.get(&fqn.into()) {
                    found_locations.extend(entries.iter().map(|e| e.location.clone()));
                }
            }

            if !found_locations.is_empty() {
                return Some(found_locations);
            }
        }
    }

    debug!("No definition found for {:?}", identifier);

    // If we found any locations during the search, return them
    match found_locations.is_empty() {
        true => None,
        false => Some(found_locations),
    }
}
