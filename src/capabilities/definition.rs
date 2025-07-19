use log::{debug, info};
use lsp_types::{Location, Position, Url};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::ancestor_chain::get_ancestor_chain;

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
use crate::indexer::entry::MethodOrigin;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_variable::{RubyVariable, RubyVariableType};

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
    let analyzer = RubyPrismAnalyzer::new(uri, content.to_string());

    let (identifier, ancestors, _scope_stack) = analyzer.get_identifier(position);

    if let None = identifier {
        info!("No identifier found at position {:?}", position);
        return None;
    }

    info!(
        "Looking for definition of: {}->{}",
        FullyQualifiedName::from(ancestors.clone()),
        identifier.clone().unwrap(),
    );

    // Get the index and search for the definition
    let index = server.index.lock();
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
        }
        Identifier::RubyMethod(ns, method) => {
            let receiver_fqn = if ns.is_empty() {
                // Receiver-less call, use lexical scope as the receiver
                FullyQualifiedName::Constant(ancestors.clone())
            } else {
                // Call with explicit receiver
                FullyQualifiedName::Constant(ns.clone())
            };

            // For receiver-less calls, we might need to check for both instance and class methods
            // as we don't know the context of `self`. For calls with an explicit receiver, the
            // identifier visitor can determine the kind.
            let kinds_to_check = if ns.is_empty() {
                vec![MethodKind::Instance, MethodKind::Class]
            } else {
                vec![method.1.clone()]
            };

            for kind in kinds_to_check {
                let is_class_method = kind == MethodKind::Class;
                let ancestor_chain = get_ancestor_chain(&index, &receiver_fqn, is_class_method);

                debug!(
                    "Searching for {} method {:?} in ancestor chain: {:?}",
                    if is_class_method { "class" } else { "instance" },
                    &method.0,
                    ancestor_chain
                        .iter()
                        .map(|fqn| fqn.to_string())
                        .collect::<Vec<_>>(),
                );

                for ancestor_fqn in ancestor_chain {
                    let method_fqn = if is_class_method {
                        FullyQualifiedName::class_method(
                            ancestor_fqn.namespace_parts(),
                            method.clone(),
                        )
                    } else {
                        FullyQualifiedName::instance_method(
                            ancestor_fqn.namespace_parts(),
                            method.clone(),
                        )
                    };

                    if let Some(entries) = index.definitions.get(&method_fqn.into()) {
                        if !entries.is_empty() {
                            found_locations.extend(entries.iter().map(|e| e.location.clone()));
                        }
                    }
                }
            }
            // First try to search by definitions using ancestors
            let mut search_namespaces = ancestors.clone();
            while !search_namespaces.is_empty() {
                let mut combined_ns = search_namespaces.clone();
                combined_ns.extend(ns.iter().cloned());

                let search_fqn = Identifier::RubyMethod(combined_ns, method.clone());

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

            // Try to find the method with the exact namespace
            if let Some(entries) = index.methods_by_name.get(&method) {
                if !entries.is_empty() {
                    // Include all methods with Direct origin
                    for entry in entries {
                        if let EntryKind::Method { origin, .. } = &entry.kind {
                            debug!("Checking method entry: origin={:?}", origin);
                            if matches!(origin, MethodOrigin::Direct) {
                                debug!("Adding location: {:?}", entry.location);
                                found_locations.push(entry.location.clone());
                            }
                        }
                    }
                }
            }
            if !found_locations.is_empty() {
                return Some(found_locations);
            }
        }
        Identifier::RubyVariable(variable) => {
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
                return Some(found_locations);
            }
        }
    }

    info!("No definition found for {:?}", identifier);

    // If we found any locations during the search, return them
    match found_locations.is_empty() {
        true => None,
        false => Some(found_locations),
    }
}
