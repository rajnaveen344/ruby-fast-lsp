use anyhow::Result;
use std::fs;
use std::path::Path;

use tree_sitter::Parser;

/// Test the parser's ability to handle various Ruby language constructs
#[tokio::test]
async fn test_ruby_language_parsing() -> Result<()> {
    // Initialize parser
    let mut parser = Parser::new();
    let language = tree_sitter_ruby::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Ruby grammar");

    // Test parsing each fixture
    let fixtures = [
        "tests/fixtures/class_declaration.rb",
        "tests/fixtures/method_with_args.rb",
        "tests/fixtures/nested_classes.rb",
        "tests/fixtures/variables.rb",
        "tests/fixtures/control_flow.rb",
        "tests/fixtures/begin_rescue.rb",
        "tests/fixtures/blocks_and_procs.rb",
    ];

    for fixture_path in fixtures.iter() {
        let path = Path::new(fixture_path);
        let content = fs::read_to_string(path)?;

        println!("Parsing fixture: {}", path.display());
        let tree = parser.parse(&content, None).expect("Failed to parse");

        // Ensure we have a valid tree
        let root_node = tree.root_node();
        assert!(root_node.child_count() > 0, "Tree should have child nodes");
    }

    Ok(())
}

/// Test parsing a class declaration
#[tokio::test]
async fn test_parse_class() -> Result<()> {
    let mut parser = Parser::new();
    let language = tree_sitter_ruby::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Ruby grammar");

    let path = Path::new("tests/fixtures/class_declaration.rb");
    let content = fs::read_to_string(path)?;

    let tree = parser.parse(&content, None).expect("Failed to parse");
    let root_node = tree.root_node();

    // Check if we can find a class node
    let mut cursor = root_node.walk();
    let mut found_class = false;

    if cursor.goto_first_child() {
        loop {
            let node = cursor.node();
            if node.kind() == "class" {
                found_class = true;
                break;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    assert!(found_class, "Should find class definition");

    Ok(())
}

/// Test parsing nested classes
#[tokio::test]
async fn test_parse_nested_classes() -> Result<()> {
    let mut parser = Parser::new();
    let language = tree_sitter_ruby::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Ruby grammar");

    let path = Path::new("tests/fixtures/nested_classes.rb");
    let content = fs::read_to_string(path)?;

    let tree = parser.parse(&content, None).expect("Failed to parse");
    let root_node = tree.root_node();

    // Check for nested class structure (class inside class or module)
    let mut cursor = root_node.walk();
    let mut found_outer = false;

    // Find the outer class/module
    if cursor.goto_first_child() {
        loop {
            let node = cursor.node();
            if node.kind() == "class" || node.kind() == "module" {
                found_outer = true;

                // Try to find a nested class/module
                let mut inner_cursor = node.walk();
                let mut found_inner = false;

                if inner_cursor.goto_first_child() {
                    loop {
                        let inner_node = inner_cursor.node();
                        if inner_node.kind() == "class" || inner_node.kind() == "module" {
                            found_inner = true;
                            break;
                        }
                        if !inner_cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }

                assert!(found_inner, "Should find nested class or module");
                break;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    assert!(found_outer, "Should find outer class or module");

    Ok(())
}
