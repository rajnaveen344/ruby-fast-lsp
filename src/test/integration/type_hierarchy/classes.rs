//! Integration tests for Type Hierarchy feature.
//!
//! Tests the type hierarchy LSP feature using the `<th>` tag:
//! - `<th supertypes="...">` - check ancestors/supertypes at cursor ($0)
//! - `<th subtypes="...">` - check descendants/subtypes at cursor ($0)
//! - Both can be combined
//!
//! The cursor `$0` should be placed on the class/module name you want to check.

use crate::test::harness::check;

// ============================================================================
// Supertype Tests (ancestors)
// ============================================================================

/// Test supertype: class with superclass
#[tokio::test]
async fn test_supertypes_simple_inheritance() {
    check(
        r#"
class Animal
end

<th supertypes="Animal">
class Dog$0 < Animal
end
</th>
"#,
    )
    .await;
}

/// Test supertypes: class with included modules
#[tokio::test]
async fn test_supertypes_with_includes() {
    check(
        r#"
module Walkable
end

module Swimmable
end

<th supertypes="Walkable,Swimmable">
class Duck$0
  include Walkable
  include Swimmable
end
</th>
"#,
    )
    .await;
}

/// Test supertypes: class with superclass and includes
#[tokio::test]
async fn test_supertypes_inheritance_and_includes() {
    check(
        r#"
module Comparable
end

class Vehicle
end

<th supertypes="Vehicle,Comparable">
class Car$0 < Vehicle
  include Comparable
end
</th>
"#,
    )
    .await;
}

/// Test supertypes: module with included modules
#[tokio::test]
async fn test_supertypes_module_includes() {
    check(
        r#"
module Loggable
end

<th supertypes="Loggable">
module Traceable$0
  include Loggable
end
</th>
"#,
    )
    .await;
}

/// Test supertypes: class with prepended module
#[tokio::test]
async fn test_supertypes_with_prepend() {
    check(
        r#"
module Logging
end

<th supertypes="Logging">
class Service$0
  prepend Logging
end
</th>
"#,
    )
    .await;
}

/// Test supertypes: class with extended module
#[tokio::test]
async fn test_supertypes_with_extend() {
    check(
        r#"
module ClassMethods
end

<th supertypes="ClassMethods">
class User$0
  extend ClassMethods
end
</th>
"#,
    )
    .await;
}

// ============================================================================
// Subtype Tests (descendants)
// ============================================================================

/// Test subtypes: class with subclasses
#[tokio::test]
async fn test_subtypes_simple_inheritance() {
    check(
        r#"
<th subtypes="Dog,Cat">
class Animal$0
end
</th>

class Dog < Animal
end

class Cat < Animal
end
"#,
    )
    .await;
}

/// Test subtypes: module included by classes
#[tokio::test]
async fn test_subtypes_module_included_by() {
    check(
        r#"
<th subtypes="UserService,OrderService">
module Loggable$0
end
</th>

class UserService
  include Loggable
end

class OrderService
  include Loggable
end
"#,
    )
    .await;
}

/// Test subtypes: module prepended by classes
#[tokio::test]
async fn test_subtypes_module_prepended_by() {
    check(
        r#"
<th subtypes="Handler">
module Validation$0
end
</th>

class Handler
  prepend Validation
end
"#,
    )
    .await;
}

/// Test subtypes: module extended by classes
#[tokio::test]
async fn test_subtypes_module_extended_by() {
    check(
        r#"
<th subtypes="Config">
module Configurable$0
end
</th>

class Config
  extend Configurable
end
"#,
    )
    .await;
}

// ============================================================================
// Combined Tests (both supertypes and subtypes)
// ============================================================================

/// Test both supertypes and subtypes
#[tokio::test]
async fn test_combined_supertypes_and_subtypes() {
    check(
        r#"
class Mammal
end

<th supertypes="Mammal" subtypes="Dog,Wolf">
class Canine$0 < Mammal
end
</th>

class Dog < Canine
end

class Wolf < Canine
end
"#,
    )
    .await;
}

/// Test complex hierarchy with mixins
#[tokio::test]
async fn test_complex_hierarchy_with_mixins() {
    check(
        r#"
module Comparable
end

module Enumerable
end

<th supertypes="Comparable" subtypes="Car">
class Vehicle$0
  include Comparable
end
</th>

class Car < Vehicle
  include Enumerable
end
"#,
    )
    .await;
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test class with no supertypes (root class)
#[tokio::test]
async fn test_no_supertypes() {
    check(
        r#"
<th supertypes="">
class BaseClass$0
end
</th>
"#,
    )
    .await;
}

/// Test class with no subtypes (leaf class)
#[tokio::test]
async fn test_no_subtypes() {
    check(
        r#"
class Animal
end

<th supertypes="Animal" subtypes="">
class Dog$0 < Animal
end
</th>
"#,
    )
    .await;
}

/// Test nested class hierarchy
#[tokio::test]
async fn test_nested_class_hierarchy() {
    check(
        r#"
module Outer
  class Base
  end
end

<th supertypes="Base">
class Child$0 < Outer::Base
end
</th>
"#,
    )
    .await;
}

/// Test deep inheritance chain - intermediate class
#[tokio::test]
async fn test_deep_inheritance_chain() {
    check(
        r#"
class Organism
end

<th supertypes="Organism" subtypes="Dog">
class Mammal$0 < Organism
end
</th>

class Dog < Mammal
end
"#,
    )
    .await;
}

// ============================================================================
// Unit Tests (for the check module integration)
// ============================================================================

#[cfg(test)]
mod check_harness_tests {
    use super::*;

    #[tokio::test]
    async fn test_check_type_hierarchy_supertypes() {
        check(
            r#"
class Parent
end

<th supertypes="Parent">
class Child$0 < Parent
end
</th>
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_type_hierarchy_subtypes() {
        check(
            r#"
<th subtypes="Child">
class Parent$0
end
</th>

class Child < Parent
end
"#,
        )
        .await;
    }
}

// ============================================================================
// Cross-File Mixin Tests
// ============================================================================
//
// These tests verify that when a class is reopened in a different file
// and includes modules there, the type hierarchy:
// 1. Shows all includes from all files
// 2. Adds a warning indicator for includes from non-primary files

#[cfg(test)]
mod cross_file_tests {
    use crate::capabilities::type_hierarchy;
    use crate::test::harness::setup_with_multi_file_fixture;
    use tower_lsp::lsp_types::{
        PartialResultParams, Position, TextDocumentIdentifier, TextDocumentPositionParams,
        TypeHierarchyPrepareParams, TypeHierarchySupertypesParams, WorkDoneProgressParams,
    };

    /// Helper to find a class/module by searching through lines
    async fn find_type_at_name(
        server: &crate::server::RubyLanguageServer,
        uri: &tower_lsp::lsp_types::Url,
        content: &str,
        type_name: &str,
    ) -> tower_lsp::lsp_types::TypeHierarchyItem {
        // Find the line containing "class TypeName" or "module TypeName"
        for (line_idx, line) in content.lines().enumerate() {
            let class_pattern = format!("class {}", type_name);
            let module_pattern = format!("module {}", type_name);

            let char_pos = if let Some(pos) = line.find(&class_pattern) {
                Some(pos + 6) // "class " is 6 chars
            } else if let Some(pos) = line.find(&module_pattern) {
                Some(pos + 7) // "module " is 7 chars
            } else {
                None
            };

            if let Some(char_pos) = char_pos {
                let prepare_params = TypeHierarchyPrepareParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        position: Position {
                            line: line_idx as u32,
                            character: char_pos as u32,
                        },
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                };

                if let Some(items) =
                    type_hierarchy::handle_prepare_type_hierarchy(server, prepare_params).await
                {
                    if !items.is_empty() && items[0].name == type_name {
                        return items.into_iter().next().unwrap();
                    }
                }
            }
        }

        panic!("Could not find type '{}' in content", type_name);
    }

    /// Test that includes from a reopened class in a different file
    /// are collected and show the cross-file warning
    #[tokio::test]
    async fn test_cross_file_includes_show_warning() {
        // File 1: Define class with one include
        let file1_content = r#"module ModuleA
end

module ModuleB
end

class MyClass
  include ModuleA
end
"#;
        let file1 = ("main.rb", file1_content);

        // File 2: Reopen class and add another include
        let file2 = (
            "extension.rb",
            r#"class MyClass
  include ModuleB
end
"#,
        );

        let (server, uris) = setup_with_multi_file_fixture(&[file1, file2]).await;

        // Find MyClass
        let item = find_type_at_name(&server, &uris[0], file1_content, "MyClass").await;
        assert_eq!(item.name, "MyClass");

        // Get supertypes
        let supertypes_params = TypeHierarchySupertypesParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let supertypes = type_hierarchy::handle_supertypes(&server, supertypes_params)
            .await
            .expect("Should get supertypes");

        // Should have both ModuleA and ModuleB
        let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"ModuleA"),
            "Should include ModuleA. Got: {:?}",
            names
        );
        assert!(
            names.contains(&"ModuleB"),
            "Should include ModuleB. Got: {:?}",
            names
        );

        // Verify details - ModuleA should NOT have warning (same file)
        // ModuleB SHOULD have warning (different file)
        let details: Vec<(&str, &str)> = supertypes
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.detail.as_ref().map(|d| d.as_str()).unwrap_or(""),
                )
            })
            .collect();

        let module_a_detail = details.iter().find(|(n, _)| *n == "ModuleA").unwrap().1;
        let module_b_detail = details.iter().find(|(n, _)| *n == "ModuleB").unwrap().1;

        assert!(
            !module_a_detail.contains("⚠️"),
            "ModuleA (same file) should NOT have warning. Got: {}",
            module_a_detail
        );
        assert!(
            module_b_detail.contains("⚠️"),
            "ModuleB (different file) should have warning. Got: {}",
            module_b_detail
        );
        assert!(
            module_b_detail.contains("extension.rb"),
            "ModuleB warning should mention source file. Got: {}",
            module_b_detail
        );
    }

    /// Test that module reopening across files also works
    #[tokio::test]
    async fn test_module_cross_file_includes() {
        let file1_content = r#"module MixinOne
end

module MixinTwo
end

module API
  include MixinOne
end
"#;
        let file1 = ("base_module.rb", file1_content);

        let file2 = (
            "api_extension.rb",
            r#"module API
  include MixinTwo
end
"#,
        );

        let (server, uris) = setup_with_multi_file_fixture(&[file1, file2]).await;

        // Find API module
        let item = find_type_at_name(&server, &uris[0], file1_content, "API").await;

        // Get supertypes
        let supertypes_params = TypeHierarchySupertypesParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let supertypes = type_hierarchy::handle_supertypes(&server, supertypes_params)
            .await
            .expect("Should get supertypes");

        // Should have both mixins
        let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"MixinOne"),
            "Should include MixinOne. Got: {:?}",
            names
        );
        assert!(
            names.contains(&"MixinTwo"),
            "Should include MixinTwo. Got: {:?}",
            names
        );

        // MixinTwo should have warning
        let mixin_two_detail = supertypes
            .iter()
            .find(|s| s.name == "MixinTwo")
            .unwrap()
            .detail
            .as_ref()
            .unwrap();

        assert!(
            mixin_two_detail.contains("⚠️"),
            "MixinTwo should have warning. Got: {}",
            mixin_two_detail
        );
    }

    /// Test all mixin types (include, prepend, extend) across files
    #[tokio::test]
    async fn test_all_mixin_types_cross_file() {
        let file1_content = r#"module IncludeMe
end

module PrependMe
end

module ExtendMe
end

class Widget
  include IncludeMe
end
"#;
        let file1 = ("primary.rb", file1_content);

        let file2 = (
            "secondary.rb",
            r#"class Widget
  prepend PrependMe
  extend ExtendMe
end
"#,
        );

        let (server, uris) = setup_with_multi_file_fixture(&[file1, file2]).await;

        // Find Widget
        let item = find_type_at_name(&server, &uris[0], file1_content, "Widget").await;

        // Get supertypes
        let supertypes_params = TypeHierarchySupertypesParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let supertypes = type_hierarchy::handle_supertypes(&server, supertypes_params)
            .await
            .expect("Should get supertypes");

        // Should have all three
        let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"IncludeMe"), "Should have IncludeMe");
        assert!(names.contains(&"PrependMe"), "Should have PrependMe");
        assert!(names.contains(&"ExtendMe"), "Should have ExtendMe");

        // IncludeMe should NOT have warning (same file)
        let include_detail = supertypes
            .iter()
            .find(|s| s.name == "IncludeMe")
            .unwrap()
            .detail
            .as_ref()
            .unwrap();
        assert!(
            !include_detail.contains("⚠️"),
            "IncludeMe should not have warning"
        );

        // PrependMe and ExtendMe SHOULD have warning (different file)
        let prepend_detail = supertypes
            .iter()
            .find(|s| s.name == "PrependMe")
            .unwrap()
            .detail
            .as_ref()
            .unwrap();
        assert!(
            prepend_detail.contains("⚠️") && prepend_detail.contains("secondary.rb"),
            "PrependMe should have warning with filename. Got: {}",
            prepend_detail
        );

        let extend_detail = supertypes
            .iter()
            .find(|s| s.name == "ExtendMe")
            .unwrap()
            .detail
            .as_ref()
            .unwrap();
        assert!(
            extend_detail.contains("⚠️") && extend_detail.contains("secondary.rb"),
            "ExtendMe should have warning with filename. Got: {}",
            extend_detail
        );
    }

    /// Test that unresolved mixins (from external gems/stdlib) show warning
    #[tokio::test]
    async fn test_unresolved_mixin_shows_warning() {
        let file1_content = r#"class MyController
  include ActionController::Base
end
"#;
        let file1 = ("controller.rb", file1_content);

        let (server, uris) = setup_with_multi_file_fixture(&[file1]).await;

        // Find MyController
        let item = find_type_at_name(&server, &uris[0], file1_content, "MyController").await;

        // Get supertypes
        let supertypes_params = TypeHierarchySupertypesParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let supertypes = type_hierarchy::handle_supertypes(&server, supertypes_params)
            .await
            .expect("Should get supertypes");

        // Should have the unresolved include with warning
        assert!(!supertypes.is_empty(), "Should show unresolved include");

        let base_detail = supertypes
            .iter()
            .find(|s| s.name == "Base")
            .expect("Should have Base entry")
            .detail
            .as_ref()
            .unwrap();

        assert!(
            base_detail.contains("❓") && base_detail.contains("definition not found"),
            "Unresolved mixin should have 'definition not found' warning. Got: {}",
            base_detail
        );
    }

    /// Test relation labels are present in detail
    #[tokio::test]
    async fn test_relation_labels_present() {
        let file1_content = r#"module Includable
end

module Prependable
end

module Extendable
end

class Parent
end

class Child < Parent
  include Includable
  prepend Prependable
  extend Extendable
end
"#;
        let file1 = ("test.rb", file1_content);

        let (server, uris) = setup_with_multi_file_fixture(&[file1]).await;

        // Find Child
        let item = find_type_at_name(&server, &uris[0], file1_content, "Child").await;

        // Get supertypes
        let supertypes_params = TypeHierarchySupertypesParams {
            item: item.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let supertypes = type_hierarchy::handle_supertypes(&server, supertypes_params)
            .await
            .expect("Should get supertypes");

        // Collect details
        let details: std::collections::HashMap<&str, &str> = supertypes
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.detail.as_ref().map(|d| d.as_str()).unwrap_or(""),
                )
            })
            .collect();

        // Check relation labels are present
        assert!(
            details.get("Parent").unwrap().contains("superclass"),
            "Superclass should have 'superclass' label. Got: {}",
            details.get("Parent").unwrap()
        );
        assert!(
            details.get("Includable").unwrap().contains("include"),
            "Include should have 'include' label. Got: {}",
            details.get("Includable").unwrap()
        );
        assert!(
            details.get("Prependable").unwrap().contains("prepend"),
            "Prepend should have 'prepend' label. Got: {}",
            details.get("Prependable").unwrap()
        );
        assert!(
            details.get("Extendable").unwrap().contains("extend"),
            "Extend should have 'extend' label. Got: {}",
            details.get("Extendable").unwrap()
        );
    }
}
