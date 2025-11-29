//! YARD Type Definition Lookup
//!
//! Provides go-to-definition support for type references in YARD documentation comments.
//! When the cursor is on a type like `String` or `MyClass` in a YARD comment,
//! this module finds the definition of that class/module.

use log::info;
use tower_lsp::lsp_types::Location;

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;

/// Find definitions for a YARD type reference
///
/// This looks up the class/module definition for a type name found in YARD documentation.
/// Handles both simple types (String) and namespaced types (Foo::Bar).
pub fn find_yard_type_definitions(type_name: &str, index: &RubyIndex) -> Option<Vec<Location>> {
    info!("Looking for YARD type definition: {}", type_name);

    // Handle built-in types that won't have definitions in user code
    // but might be in stdlib stubs
    let builtins_without_definitions = ["nil", "true", "false", "void", "Boolean", "bool"];
    if builtins_without_definitions
        .iter()
        .any(|b| b.eq_ignore_ascii_case(type_name))
    {
        info!(
            "Type '{}' is a built-in without a navigable definition",
            type_name
        );
        return None;
    }

    // Parse the type name into namespace parts
    let parts: Vec<&str> = type_name.split("::").collect();
    let mut namespace = Vec::new();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        match RubyConstant::try_from(trimmed) {
            Ok(constant) => namespace.push(constant),
            Err(_) => {
                info!("Invalid constant name in type: {}", trimmed);
                return None;
            }
        }
    }

    if namespace.is_empty() {
        return None;
    }

    // Create FQN and look up in index
    let fqn = FullyQualifiedName::Constant(namespace);

    if let Some(entries) = index.get(&fqn) {
        let locations: Vec<Location> = entries
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Class { .. } | EntryKind::Module { .. }))
            .map(|e| e.location.clone())
            .collect();

        if !locations.is_empty() {
            info!(
                "Found {} definition(s) for type '{}'",
                locations.len(),
                type_name
            );
            return Some(locations);
        }
    }

    info!("No definition found for type '{}'", type_name);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_types_return_none() {
        let index = RubyIndex::new();
        assert!(find_yard_type_definitions("nil", &index).is_none());
        assert!(find_yard_type_definitions("true", &index).is_none());
        assert!(find_yard_type_definitions("false", &index).is_none());
        assert!(find_yard_type_definitions("void", &index).is_none());
        assert!(find_yard_type_definitions("Boolean", &index).is_none());
    }

    #[test]
    fn test_empty_type_returns_none() {
        let index = RubyIndex::new();
        assert!(find_yard_type_definitions("", &index).is_none());
        assert!(find_yard_type_definitions("  ", &index).is_none());
    }

    #[test]
    fn test_invalid_constant_returns_none() {
        let index = RubyIndex::new();
        // lowercase names are not valid constants
        assert!(find_yard_type_definitions("lowercase", &index).is_none());
    }
}
