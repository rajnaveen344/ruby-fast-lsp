//! Type-aware method completion
//!
//! Provides method completions based on the receiver's inferred type.
//! Uses both the Ruby index (for user-defined methods) and RBS (for built-in methods).

use parking_lot::Mutex;
use std::sync::Arc;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

use crate::indexer::ancestor_chain::get_ancestor_chain;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
use crate::indexer::index::RubyIndex;
use crate::type_inference::rbs_index::{get_rbs_class_methods, RbsMethodInfo};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

/// Find method completions for a receiver type
pub fn find_method_completions(
    index: &Arc<Mutex<RubyIndex>>,
    receiver_type: &RubyType,
    partial_method: &str,
    is_class_method: bool,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();
    let mut seen_methods = std::collections::HashSet::new();

    // Get the class name for lookups
    let class_names = get_class_names_for_type(receiver_type);

    for class_name in &class_names {
        // Get methods from RBS (built-in types)
        let rbs_methods = get_rbs_class_methods(class_name, is_class_method);
        for method_info in rbs_methods {
            // Filter by partial match
            if !method_info.name.starts_with(partial_method) {
                continue;
            }

            // Skip if already seen
            if seen_methods.contains(&method_info.name) {
                continue;
            }
            seen_methods.insert(method_info.name.clone());

            completions.push(create_method_completion_item(&method_info));
        }

        // Get methods from Ruby index (user-defined types) including ancestor chain
        let index_methods =
            get_index_methods_with_ancestors(index, class_name, partial_method, is_class_method);
        for (method_name, return_type, params) in index_methods {
            if seen_methods.contains(&method_name) {
                continue;
            }
            seen_methods.insert(method_name.clone());

            let method_info = RbsMethodInfo {
                name: method_name,
                return_type,
                is_singleton: is_class_method,
                params,
            };
            completions.push(create_method_completion_item(&method_info));
        }
    }

    // Sort by name
    completions.sort_by(|a, b| a.label.cmp(&b.label));

    completions
}

/// Get class names from a RubyType for method lookup
fn get_class_names_for_type(ruby_type: &RubyType) -> Vec<String> {
    match ruby_type {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
            if let FullyQualifiedName::Constant(parts) = fqn {
                // Return both the simple name and the FQN
                let simple_name = parts.last().map(|c| c.to_string());
                let fqn_name = parts
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                let mut names = Vec::new();
                if let Some(name) = simple_name {
                    names.push(name);
                }
                if !fqn_name.is_empty() && names.first() != Some(&fqn_name) {
                    names.push(fqn_name);
                }
                names
            } else {
                vec![]
            }
        }
        RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => {
            if let FullyQualifiedName::Constant(parts) = fqn {
                parts
                    .last()
                    .map(|c| vec![c.to_string()])
                    .unwrap_or_default()
            } else {
                vec![]
            }
        }
        RubyType::Array(_) => vec!["Array".to_string()],
        RubyType::Hash(_, _) => vec!["Hash".to_string()],
        RubyType::Union(types) => {
            // Get class names from ALL types in the union
            // This allows showing methods from all possible types
            let mut all_names = Vec::new();
            for ty in types {
                for name in get_class_names_for_type(ty) {
                    if !all_names.contains(&name) {
                        all_names.push(name);
                    }
                }
            }
            all_names
        }
        _ => vec![],
    }
}

/// Get methods from the Ruby index for a class, including methods from ancestor chain
fn get_index_methods_with_ancestors(
    index: &Arc<Mutex<RubyIndex>>,
    class_name: &str,
    partial_method: &str,
    is_class_method: bool,
) -> Vec<(String, Option<RubyType>, Vec<String>)> {
    let index = index.lock();
    let mut methods = Vec::new();
    let mut seen_methods = std::collections::HashSet::new();

    // Try to find the FQN for the class
    let class_fqn = FullyQualifiedName::try_from(class_name);

    // Get the ancestor chain for the class
    let ancestors = if let Ok(fqn) = &class_fqn {
        get_ancestor_chain(&index, fqn, is_class_method)
    } else {
        vec![]
    };

    // Collect class names to search (the class itself + all ancestors)
    let mut classes_to_search: Vec<String> = vec![class_name.to_string()];
    for ancestor in &ancestors {
        if let FullyQualifiedName::Constant(parts) = ancestor {
            let ancestor_name = parts
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if !classes_to_search.contains(&ancestor_name) {
                classes_to_search.push(ancestor_name);
            }
            // Also add the simple name
            if let Some(simple_name) = parts.last().map(|c| c.to_string()) {
                if !classes_to_search.contains(&simple_name) {
                    classes_to_search.push(simple_name);
                }
            }
        }
    }

    let method_kind = if is_class_method {
        MethodKind::Class
    } else {
        MethodKind::Instance
    };

    // Search through methods_by_name
    for (ruby_method, entries) in &index.methods_by_name {
        // Check if method name matches partial
        let method_name = ruby_method.get_name();
        if !method_name.starts_with(partial_method) {
            continue;
        }

        // Skip if already seen
        if seen_methods.contains(&method_name.to_string()) {
            continue;
        }

        // Check method kind
        if ruby_method.get_kind() != method_kind {
            continue;
        }

        // Check if method belongs to any class in our search list
        for entry in entries {
            if let EntryKind::Method {
                owner,
                return_type,
                params,
                ..
            } = &entry.kind
            {
                // Check if owner matches any class in our list
                let owner_matches = match owner {
                    FullyQualifiedName::Constant(parts) => {
                        let owner_name = parts
                            .iter()
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join("::");
                        let simple_name = parts.last().map(|c| c.to_string());

                        classes_to_search.contains(&owner_name)
                            || simple_name
                                .as_ref()
                                .map(|s| classes_to_search.contains(s))
                                .unwrap_or(false)
                    }
                    _ => false,
                };

                if owner_matches {
                    seen_methods.insert(method_name.to_string());
                    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                    methods.push((method_name.to_string(), return_type.clone(), param_names));
                    break;
                }
            }
        }
    }

    methods
}

/// Create a completion item for a method
fn create_method_completion_item(method_info: &RbsMethodInfo) -> CompletionItem {
    let return_type_str = method_info
        .return_type
        .as_ref()
        .map(|t| format!(" -> {}", t))
        .unwrap_or_default();

    let params_str = if method_info.params.is_empty() {
        String::new()
    } else {
        format!(
            "({})",
            method_info
                .params
                .iter()
                .filter(|p| !p.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let detail = format!("{}{}{}", method_info.name, params_str, return_type_str);

    let documentation = method_info.return_type.as_ref().map(|rt| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("**Returns:** `{}`", rt),
        })
    });

    CompletionItem {
        label: method_info.name.clone(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(detail),
        documentation,
        insert_text: Some(method_info.name.clone()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_class_names_for_string_type() {
        let string_type = RubyType::string();
        let names = get_class_names_for_type(&string_type);
        assert!(names.contains(&"String".to_string()));
    }

    #[test]
    fn test_get_class_names_for_array_type() {
        let array_type = RubyType::Array(vec![RubyType::integer()]);
        let names = get_class_names_for_type(&array_type);
        assert_eq!(names, vec!["Array".to_string()]);
    }

    #[test]
    fn test_get_class_names_for_hash_type() {
        let hash_type = RubyType::Hash(vec![RubyType::symbol()], vec![RubyType::string()]);
        let names = get_class_names_for_type(&hash_type);
        assert_eq!(names, vec!["Hash".to_string()]);
    }

    #[test]
    fn test_find_string_methods() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let string_type = RubyType::string();

        let completions = find_method_completions(&index, &string_type, "", false);

        // Should have methods from RBS
        assert!(!completions.is_empty(), "Should have string methods");

        // Check for common string methods
        let method_names: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(
            method_names.contains(&"length"),
            "Should have length method"
        );
        assert!(
            method_names.contains(&"upcase"),
            "Should have upcase method"
        );
    }

    #[test]
    fn test_find_methods_with_partial() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let string_type = RubyType::string();

        let completions = find_method_completions(&index, &string_type, "up", false);

        // Should only have methods starting with "up"
        for completion in &completions {
            assert!(
                completion.label.starts_with("up"),
                "Method {} should start with 'up'",
                completion.label
            );
        }
    }

    #[test]
    fn test_method_completion_item_has_return_type() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let string_type = RubyType::string();

        let completions = find_method_completions(&index, &string_type, "length", false);

        // Find the length method
        let length_completion = completions.iter().find(|c| c.label == "length");
        assert!(length_completion.is_some(), "Should have length method");

        let length = length_completion.unwrap();
        assert!(length.detail.is_some(), "Length method should have detail");
        let detail = length.detail.as_ref().unwrap();
        assert!(
            detail.contains("Integer"),
            "Length detail should mention Integer return type, got: {}",
            detail
        );
    }

    #[test]
    fn test_union_type_completion_includes_all_types() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        // Create a union type: String | Integer
        let union_type = RubyType::union(vec![RubyType::string(), RubyType::integer()]);

        let completions = find_method_completions(&index, &union_type, "", false);

        let method_names: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

        // Should have String methods
        assert!(
            method_names.contains(&"upcase"),
            "Should have String#upcase method"
        );
        assert!(
            method_names.contains(&"downcase"),
            "Should have String#downcase method"
        );

        // Should also have Integer methods
        assert!(
            method_names.contains(&"abs"),
            "Should have Integer#abs method"
        );
        assert!(
            method_names.contains(&"times"),
            "Should have Integer#times method"
        );

        // Common methods should only appear once
        let length_count = method_names.iter().filter(|&&m| m == "to_s").count();
        assert_eq!(length_count, 1, "to_s should only appear once");
    }
}
