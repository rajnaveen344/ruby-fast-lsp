use tree_sitter::{Node, Parser, Tree};

fn parse_code(code: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_ruby::language())
        .expect("Error loading Ruby grammar");
    parser.parse(code, None).expect("Failed to parse code")
}

fn print_node_info(node: Node, code: &str, indent: usize) {
    let indent_str = " ".repeat(indent);
    let node_text = node.utf8_text(code.as_bytes()).unwrap_or("[INVALID UTF8]");

    println!("{}Node: {} - '{}'", indent_str, node.kind(), node_text);

    // Print field-based children
    for i in 0..node.child_count() {
        if let Some(field_name) = node.field_name_for_child(i) {
            if let Some(child) = node.child(i) {
                let child_text = child.utf8_text(code.as_bytes()).unwrap_or("[INVALID UTF8]");
                println!(
                    "{}  Field '{}': {} - '{}'",
                    indent_str,
                    field_name,
                    child.kind(),
                    child_text
                );
            }
        }
    }

    // Recursively print children
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            print_node_info(child, code, indent + 2);
        }
    }
}

// Find nodes matching a specific kind
fn find_nodes_by_kind<'a>(node: Node<'a>, kind: &str, results: &mut Vec<Node<'a>>) {
    if node.kind() == kind {
        results.push(node);
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            find_nodes_by_kind(child, kind, results);
        }
    }
}

#[test]
fn test_nested_scope_resolution() {
    let code = r#"
    class Outer
      def outer_method
        puts "In outer method"
      end

      class Inner
        def inner_method
          puts "In inner method"
        end

        class VeryInner
          def very_inner_method
            puts "In very inner method"
          end
        end
      end
    end

    outer = Outer.new
    outer.outer_method

    inner = Outer::Inner.new
    inner.inner_method

    very_inner = Outer::Inner::VeryInner.new
    very_inner.very_inner_method
    "#;

    let tree = parse_code(code);

    // Find all scope_resolution nodes
    let mut scope_resolution_nodes = Vec::new();
    find_nodes_by_kind(
        tree.root_node(),
        "scope_resolution",
        &mut scope_resolution_nodes,
    );

    println!(
        "Found {} scope_resolution nodes",
        scope_resolution_nodes.len()
    );

    // Examine each scope resolution node
    for (i, node) in scope_resolution_nodes.iter().enumerate() {
        println!("\nScope Resolution Node #{}: ", i + 1);
        print_node_info(*node, code, 0);
    }

    // Specifically look at the double-nested scope resolution (Outer::Inner::VeryInner)
    if let Some(node) = scope_resolution_nodes.last() {
        println!("\nDetailed analysis of Outer::Inner::VeryInner:");
        print_node_info(*node, code, 0);
    }
}
