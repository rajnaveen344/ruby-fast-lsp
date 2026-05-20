//! Type-aware method completion
//!
//! Provides method completions based on the receiver's inferred type.
//! User-defined methods are resolved through `query::completion`; this module
//! only supplies RBS-backed built-in method completions.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::inference::rbs::{get_rbs_class_methods, RbsMethodInfo};
use ruby_analysis::inference::RubyType;

pub fn find_rbs_method_completions(
    receiver_type: &RubyType,
    partial_method: &str,
    kind: NamespaceKind,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();
    let mut seen_methods = std::collections::HashSet::new();
    let is_singleton = kind == NamespaceKind::Singleton;

    let class_names = get_class_names_for_type(receiver_type);

    for class_name in &class_names {
        let rbs_methods = get_rbs_class_methods(class_name, is_singleton);
        for method_info in rbs_methods {
            if !method_info.name.starts_with(partial_method) {
                continue;
            }

            if seen_methods.contains(&method_info.name) {
                continue;
            }
            seen_methods.insert(method_info.name.clone());

            completions.push(create_method_completion_item(&method_info));
        }
    }

    // For singleton methods on classes, include Class/Module instance methods from RBS.
    // When you call `User.new`, `new` is an instance method of `Class`
    // (since `User` is an instance of `Class`).
    if is_singleton {
        for rbs_class in &["Class", "Module"] {
            let class_methods = get_rbs_class_methods(rbs_class, false);
            for method_info in class_methods {
                if !method_info.name.starts_with(partial_method) {
                    continue;
                }
                if seen_methods.contains(&method_info.name) {
                    continue;
                }
                seen_methods.insert(method_info.name.clone());
                completions.push(create_method_completion_item(&method_info));
            }
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
            let parts = match fqn {
                FullyQualifiedName::Namespace(parts, _) => parts,
                FullyQualifiedName::Constant(parts) => parts,
                _ => return vec![],
            };
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
        }
        RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => {
            let parts = match fqn {
                FullyQualifiedName::Namespace(parts, _) => parts,
                FullyQualifiedName::Constant(parts) => parts,
                _ => return vec![],
            };
            parts
                .last()
                .map(|c| vec![c.to_string()])
                .unwrap_or_default()
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
        let string_type = RubyType::string();

        let completions = find_rbs_method_completions(&string_type, "", NamespaceKind::Instance);

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
        let string_type = RubyType::string();

        let completions = find_rbs_method_completions(&string_type, "up", NamespaceKind::Instance);

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
        let string_type = RubyType::string();

        let completions =
            find_rbs_method_completions(&string_type, "length", NamespaceKind::Instance);

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
        // Create a union type: String | Integer
        let union_type = RubyType::union(vec![RubyType::string(), RubyType::integer()]);

        let completions = find_rbs_method_completions(&union_type, "", NamespaceKind::Instance);

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
