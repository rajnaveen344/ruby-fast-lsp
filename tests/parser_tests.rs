use ruby_fast_lsp::parser::{RubyParser, document::RubyDocument};
use pretty_assertions::assert_eq;

#[test]
fn test_parser_initialization() {
    let parser = RubyParser::new();
    assert!(parser.is_ok(), "Parser initialization should succeed");
    
    let parser = parser.unwrap();
    assert!(parser.has_grammar(), "Parser should have Ruby grammar loaded");
}

#[test]
fn test_parser_parse_valid_ruby() {
    let parser = RubyParser::new().unwrap();
    let source = "def hello_world\n  puts 'Hello, World!'\nend";
    
    let tree = parser.parse(source);
    assert!(tree.is_some(), "Parser should produce a tree for valid Ruby");
    
    let tree = tree.unwrap();
    let root = tree.root_node();
    assert!(!root.has_error(), "Valid Ruby code should not have syntax errors");
    
    // Check that the tree structure is as expected
    assert_eq!(root.kind(), "program", "Root node should be a program");
}

#[test]
fn test_parser_parse_invalid_ruby() {
    let parser = RubyParser::new().unwrap();
    let source = "def hello_world\n  puts 'Hello, World!'\n"; // Missing 'end'
    
    let tree = parser.parse(source);
    // Tree-sitter should still produce a tree even with syntax errors
    assert!(tree.is_some(), "Parser should produce a tree even for invalid Ruby");
    
    let tree = tree.unwrap();
    
    // Instead of looking for specific error nodes, let's check the overall structure
    // A valid method definition should have a specific structure
    // If the 'end' is missing, the structure will be different
    
    let root = tree.root_node();
    let mut cursor = tree.root_node().walk();
    let children = root.children(&mut cursor);
    
    // Check if the tree structure is as expected for a valid method
    // For an invalid method (missing 'end'), this should fail
    let mut is_valid_method = false;
    
    for node in children {
        if node.kind() == "method" {
            // A valid method should have a specific structure
            // Check if it has all expected children
            let mut method_cursor = node.walk();
            let method_children = node.children(&mut method_cursor);
            let method_children_vec: Vec<_> = method_children.collect();
            
            // A valid method should have at least 3 children (def, name, body, end)
            if method_children_vec.len() >= 3 {
                // Check if the last child is 'end'
                if let Some(last_child) = method_children_vec.last() {
                    if last_child.kind() == "end" {
                        is_valid_method = true;
                        break;
                    }
                }
            }
        }
    }
    
    // For invalid Ruby (missing 'end'), the method should not be valid
    assert!(!is_valid_method, "Invalid Ruby code (missing 'end') should not have a valid method structure");
}

#[test]
fn test_document_operations() {
    let content = "puts 'Hello'".to_string();
    let version = 1;
    
    let mut doc = RubyDocument::new(content.clone(), version);
    
    assert_eq!(doc.get_content(), content, "Document content should match");
    assert_eq!(doc.get_version(), version, "Document version should match");
    
    // Test update
    let new_content = "puts 'Updated'".to_string();
    let new_version = 2;
    doc.update_content(new_content.clone(), new_version);
    
    assert_eq!(doc.get_content(), new_content, "Updated content should match");
    assert_eq!(doc.get_version(), new_version, "Updated version should match");
}
