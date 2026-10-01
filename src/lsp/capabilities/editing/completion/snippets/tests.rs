use super::*;
use tower_lsp::lsp_types::{CompletionItemKind, InsertTextFormat};

#[test]
fn test_get_all_snippets_returns_expected_count() {
    let snippets = RubySnippets::get_all_snippets();
    // We should have at least 30 snippets (after removing Rails, attr_*, and pattern snippets)
    assert!(
        snippets.len() >= 30,
        "Expected at least 30 snippets, got {}",
        snippets.len()
    );
}

#[test]
fn test_get_matching_snippets_empty_prefix_returns_all() {
    let all_snippets = RubySnippets::get_all_snippets();
    let matching_snippets = RubySnippets::get_matching_snippets("");
    assert_eq!(all_snippets.len(), matching_snippets.len());
}

#[test]
fn test_get_matching_snippets_each_prefix() {
    let matching_snippets = RubySnippets::get_matching_snippets("each");

    // Should find both each and each_with_index snippets
    assert!(
        matching_snippets.len() >= 2,
        "Expected at least 2 'each' snippets, got {}",
        matching_snippets.len()
    );

    // Check that we have the basic snippets
    let labels: Vec<&String> = matching_snippets.iter().map(|s| &s.label).collect();
    assert!(
        labels.contains(&&"each".to_string()),
        "Should contain 'each' snippet"
    );
    assert!(
        labels.contains(&&"each_with_index".to_string()),
        "Should contain 'each_with_index' snippet"
    );
}

#[test]
fn test_contextual_each_snippet_in_method_context() {
    // Test that in method call context, iterator snippets are modified
    let method_context_snippets =
        RubySnippets::get_matching_snippets_with_context("each", SnippetContext::MethodCall);
    let general_context_snippets =
        RubySnippets::get_matching_snippets_with_context("each", SnippetContext::General);

    // Find the each snippet in both contexts
    let method_each = method_context_snippets
        .iter()
        .find(|s| s.label == "each")
        .unwrap();
    let general_each = general_context_snippets
        .iter()
        .find(|s| s.label == "each")
        .unwrap();

    // Method context should not include collection placeholder
    assert!(!method_each
        .insert_text
        .as_ref()
        .unwrap()
        .contains("${1:collection}"));
    assert!(method_each
        .insert_text
        .as_ref()
        .unwrap()
        .starts_with("each"));

    // General context should include collection placeholder
    assert!(general_each
        .insert_text
        .as_ref()
        .unwrap()
        .contains("${1:collection}"));
}

#[test]
fn test_contextual_map_snippet_in_method_context() {
    // Test that map snippet is properly modified in method context
    let method_context_snippets =
        RubySnippets::get_matching_snippets_with_context("map", SnippetContext::MethodCall);
    let map_snippet = method_context_snippets
        .iter()
        .find(|s| s.label == "map")
        .unwrap();

    assert!(!map_snippet
        .insert_text
        .as_ref()
        .unwrap()
        .contains("${1:collection}"));
    assert!(map_snippet.insert_text.as_ref().unwrap().starts_with("map"));
}

#[test]
fn test_get_matching_snippets_case_insensitive() {
    let matching_snippets_lower = RubySnippets::get_matching_snippets("each");
    let matching_snippets_upper = RubySnippets::get_matching_snippets("EACH");
    let matching_snippets_mixed = RubySnippets::get_matching_snippets("Each");

    assert_eq!(matching_snippets_lower.len(), matching_snippets_upper.len());
    assert_eq!(matching_snippets_lower.len(), matching_snippets_mixed.len());
}

#[test]
fn test_get_matching_snippets_partial_match() {
    let matching_snippets = RubySnippets::get_matching_snippets("sel");

    // Should find select snippets
    let labels: Vec<&String> = matching_snippets.iter().map(|s| &s.label).collect();
    assert!(
        labels.contains(&&"select".to_string()),
        "Should contain 'select' snippet"
    );
}

#[test]
fn test_get_matching_snippets_filter_text_contains() {
    let matching_snippets = RubySnippets::get_matching_snippets("i");

    // Should find snippets where filter_text contains "i"
    let labels: Vec<&String> = matching_snippets.iter().map(|s| &s.label).collect();
    assert!(
        labels.contains(&&"times".to_string()),
        "Should contain 'times' snippet (filter_text contains 'i')"
    );
    assert!(
        labels.contains(&&"while".to_string()),
        "Should contain 'while' snippet (filter_text contains 'i')"
    );
}

#[test]
fn test_snippet_properties() {
    let snippet = RubySnippets::if_snippet();

    // All snippets should have these basic properties
    assert!(!snippet.label.is_empty());
    assert_eq!(snippet.kind, Some(CompletionItemKind::SNIPPET));
    assert_eq!(snippet.insert_text_format, Some(InsertTextFormat::SNIPPET));
    assert!(snippet.insert_text.is_some());
    assert!(snippet.detail.is_some());
    assert!(snippet.documentation.is_some());
    assert!(snippet.sort_text.is_some());
}

#[test]
fn test_rspec_snippets_present() {
    let all_snippets = RubySnippets::get_all_snippets();
    let labels: Vec<&String> = all_snippets.iter().map(|s| &s.label).collect();

    // Check for RSpec-specific snippets
    assert!(
        labels.contains(&&"describe".to_string()),
        "Should contain 'describe' snippet"
    );
    assert!(
        labels.contains(&&"it".to_string()),
        "Should contain 'it' snippet"
    );
    assert!(
        labels.contains(&&"context".to_string()),
        "Should contain 'context' snippet"
    );
    assert!(
        labels.contains(&&"before".to_string()),
        "Should contain 'before' snippet"
    );
    assert!(
        labels.contains(&&"after".to_string()),
        "Should contain 'after' snippet"
    );
}

#[test]
fn test_control_flow_snippets_present() {
    let all_snippets = RubySnippets::get_all_snippets();
    let labels: Vec<&String> = all_snippets.iter().map(|s| &s.label).collect();

    // Check for control flow snippets
    assert!(
        labels.contains(&&"if".to_string()),
        "Should contain 'if' snippet"
    );
    assert!(
        labels.contains(&&"unless".to_string()),
        "Should contain 'unless' snippet"
    );
    assert!(
        labels.contains(&&"while".to_string()),
        "Should contain 'while' snippet"
    );
    assert!(
        labels.contains(&&"until".to_string()),
        "Should contain 'until' snippet"
    );
    assert!(
        labels.contains(&&"case when".to_string()),
        "Should contain 'case when' snippet"
    );
    assert!(
        labels.contains(&&"for".to_string()),
        "Should contain 'for' snippet"
    );
    assert!(
        labels.contains(&&"loop".to_string()),
        "Should contain 'loop' snippet"
    );
}

#[test]
fn test_method_definition_snippets_present() {
    let all_snippets = RubySnippets::get_all_snippets();
    let labels: Vec<&String> = all_snippets.iter().map(|s| &s.label).collect();

    // Check for method and class definition snippets
    assert!(
        labels.contains(&&"def".to_string()),
        "Should contain 'def' snippet"
    );
    assert!(
        labels.contains(&&"class".to_string()),
        "Should contain 'class' snippet"
    );
    assert!(
        labels.contains(&&"module".to_string()),
        "Should contain 'module' snippet"
    );
}

#[test]
fn test_exception_handling_snippets_present() {
    let all_snippets = RubySnippets::get_all_snippets();
    let labels: Vec<&String> = all_snippets.iter().map(|s| &s.label).collect();

    // Check for exception handling snippets
    assert!(
        labels.contains(&&"begin rescue".to_string()),
        "Should contain 'begin rescue' snippet"
    );
    assert!(
        labels.contains(&&"begin rescue ensure".to_string()),
        "Should contain 'begin rescue ensure' snippet"
    );
    assert!(
        labels.contains(&&"rescue".to_string()),
        "Should contain 'rescue' snippet"
    );
}

#[test]
fn test_snippet_sorting() {
    let all_snippets = RubySnippets::get_all_snippets();

    // All snippets should have sort_text for proper ordering
    for snippet in &all_snippets {
        assert!(
            snippet.sort_text.is_some(),
            "Snippet '{}' should have sort_text",
            snippet.label
        );
    }
}

#[test]
fn test_context_aware_snippet_modification() {
    // Test that the same snippet behaves differently in different contexts
    let general_snippets =
        RubySnippets::get_matching_snippets_with_context("each", SnippetContext::General);
    let method_snippets =
        RubySnippets::get_matching_snippets_with_context("each", SnippetContext::MethodCall);

    let general_each = general_snippets.iter().find(|s| s.label == "each").unwrap();
    let method_each = method_snippets.iter().find(|s| s.label == "each").unwrap();

    // General context should include collection placeholder
    assert!(general_each
        .insert_text
        .as_ref()
        .unwrap()
        .contains("${1:collection}"));

    // Method context should not include collection placeholder
    assert!(!method_each
        .insert_text
        .as_ref()
        .unwrap()
        .contains("${1:collection}"));

    // Method context should start with the method name
    assert!(method_each
        .insert_text
        .as_ref()
        .unwrap()
        .starts_with("each"));
}

#[test]
fn test_determine_context_with_analyzer() {
    use ruby_analysis::indexer::RubyPrismAnalyzer;
    use tower_lsp::lsp_types::Position;

    // Test method call context (a.each)
    let content = "a = [1, 2, 3]\na.each";
    let analyzer = RubyPrismAnalyzer::new(
        crate::test::harness::fixture_uri("/test.rb"),
        content.to_string(),
    );
    let position = Position::new(1, 5); // Position at last char of "each" in "a.each"
    let (identifier, _, _, _, _) = analyzer.get_identifier_at_position(
        ruby_analysis::core::SourcePosition::new(position.line, position.character),
    );

    let context = RubySnippets::determine_context(&identifier);
    match context {
        SnippetContext::MethodCall => {}
        SnippetContext::General => {
            // If we get here, let's see what the identifier actually is
            panic!(
                "Expected MethodCall context for 'a.each', but got General. Identifier: {:?}",
                identifier
            );
        }
    }

    // Test general context (just "each")
    let content2 = "each";
    let analyzer2 = RubyPrismAnalyzer::new(
        crate::test::harness::fixture_uri("/test.rb"),
        content2.to_string(),
    );
    let position2 = Position::new(0, 3); // Position at last char of "each"
    let (identifier2, _, _, _, _) = analyzer2.get_identifier_at_position(
        ruby_analysis::core::SourcePosition::new(position2.line, position2.character),
    );

    let context2 = RubySnippets::determine_context(&identifier2);
    assert!(
        matches!(context2, SnippetContext::General),
        "Expected General context for standalone 'each'"
    );
}

#[test]
fn test_full_completion_output_with_analyzer() {
    use ruby_analysis::indexer::RubyPrismAnalyzer;
    use tower_lsp::lsp_types::Position;

    // Test method call context (a.each) - should NOT include collection placeholder
    let content = "a = [1, 2, 3]\na.each";
    let analyzer = RubyPrismAnalyzer::new(
        crate::test::harness::fixture_uri("/test.rb"),
        content.to_string(),
    );
    let position = Position::new(1, 5); // Position at last char of "each" in "a.each"
    let (identifier, _, _, _, _) = analyzer.get_identifier_at_position(
        ruby_analysis::core::SourcePosition::new(position.line, position.character),
    );

    let context = RubySnippets::determine_context(&identifier);
    let completions = RubySnippets::get_matching_snippets_with_context("each", context);

    // Find the each snippet
    let each_completion = completions.iter().find(|s| s.label == "each");
    assert!(each_completion.is_some(), "Should find 'each' completion");

    let each_snippet = each_completion.unwrap();
    let insert_text = each_snippet.insert_text.as_ref().unwrap();

    println!(
        "Method call context - each snippet insert_text: {}",
        insert_text
    );

    // In method call context, should NOT contain collection placeholder
    assert!(
        !insert_text.contains("${1:collection}"),
        "Method call context should not contain collection placeholder. Got: {}",
        insert_text
    );
    assert!(
        insert_text.starts_with("each"),
        "Method call context should start with 'each'. Got: {}",
        insert_text
    );

    // Test general context (just "each") - SHOULD include collection placeholder
    let content2 = "each";
    let analyzer2 = RubyPrismAnalyzer::new(
        crate::test::harness::fixture_uri("/test.rb"),
        content2.to_string(),
    );
    let position2 = Position::new(0, 3); // Position at last char of "each"
    let (identifier2, _, _, _, _) = analyzer2.get_identifier_at_position(
        ruby_analysis::core::SourcePosition::new(position2.line, position2.character),
    );

    let context2 = RubySnippets::determine_context(&identifier2);
    let completions2 = RubySnippets::get_matching_snippets_with_context("each", context2);

    let each_completion2 = completions2.iter().find(|s| s.label == "each");
    assert!(
        each_completion2.is_some(),
        "Should find 'each' completion in general context"
    );

    let each_snippet2 = each_completion2.unwrap();
    let insert_text2 = each_snippet2.insert_text.as_ref().unwrap();

    println!(
        "General context - each snippet insert_text: {}",
        insert_text2
    );

    // In general context, should contain collection placeholder
    assert!(
        insert_text2.contains("${1:collection}"),
        "General context should contain collection placeholder. Got: {}",
        insert_text2
    );
}
