use lsp_types::Position;
use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

pub struct RubyAnalyzer {
    parser: Parser,
    document: String,
}

impl RubyAnalyzer {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_ruby::language())
            .expect("Failed to load Ruby grammar");
        RubyAnalyzer {
            parser,
            document: String::new(),
        }
    }

    // Find the identifier node at the given position
    pub fn find_identifier_at_position(
        &mut self,
        document: &str,
        position: Position,
    ) -> Option<String> {
        // Store the document for use by other methods
        self.document = document.to_string();

        let line_starts = get_line_starts(document.as_bytes());
        let _byte_offset = position_to_offset(position, &line_starts)?;
        let tree = self.parser.parse(document, None)?;

        // Find the node at the position
        let point = Point::new(position.line as usize, position.character as usize);
        let mut cursor = tree.root_node().walk();

        let node = self.find_node_at_point(&mut cursor, point)?;

        // Get the fully qualified name for the node
        let fully_qualified_name = self.determine_fully_qualified_name(&tree, node, position);
        Some(fully_qualified_name)
    }

    // Find the node at a specific point
    fn find_node_at_point<'a>(
        &self,
        cursor: &mut TreeCursor<'a>,
        point: Point,
    ) -> Option<Node<'a>> {
        // Implementation to walk the tree and find the node at the position
        let node = cursor.node();

        // Check if this node contains the point using tree-sitter's Range
        let node_range = node.range();
        if point.row < node_range.start_point.row
            || point.row > node_range.end_point.row
            || (point.row == node_range.start_point.row
                && point.column < node_range.start_point.column)
            || (point.row == node_range.end_point.row && point.column > node_range.end_point.column)
        {
            return None;
        }

        // Special handling for instance variables (@var)
        // If we're at position '@' and the next character would be part of an instance variable,
        // expand our search to include the entire instance variable
        if let Some(instance_var_node) = self.check_for_instance_variable(&node, point) {
            return Some(instance_var_node);
        }

        // Special handling for block parameters
        if node.kind() == "block_parameters" {
            for i in 0..node.named_child_count() {
                if let Some(param) = node.named_child(i) {
                    let param_range = param.range();
                    if point.row == param_range.start_point.row
                        && point.column >= param_range.start_point.column
                        && point.column <= param_range.end_point.column
                    {
                        return Some(param);
                    }
                }
            }
        } else if node.kind() == "block" {
            if let Some(params) = node.child_by_field_name("parameters") {
                for i in 0..params.named_child_count() {
                    if let Some(param) = params.named_child(i) {
                        let param_range = param.range();
                        if point.row == param_range.start_point.row
                            && point.column >= param_range.start_point.column
                            && point.column <= param_range.end_point.column
                        {
                            return Some(param);
                        }
                    }
                }
            }
        }

        // Check if any child contains the point
        if cursor.goto_first_child() {
            loop {
                if let Some(child_node) = self.find_node_at_point(cursor, point) {
                    return Some(child_node);
                }

                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }

        // If no child contains the point, then this node is the smallest containing node
        Some(node)
    }

    // Helper method to check if a point is at the start of an instance variable
    fn check_for_instance_variable<'a>(&self, node: &Node<'a>, point: Point) -> Option<Node<'a>> {
        // Check if we're on the @ symbol of an instance variable
        if node.kind() == "instance_variable" {
            return Some(*node);
        }

        // If we're in a parent node that contains an instance variable that starts at this point
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "instance_variable" {
                    let range = child.range();

                    // If the point is at the start of the instance variable or one character before it
                    if (point.row == range.start_point.row
                        && (point.column == range.start_point.column
                            || point.column + 1 == range.start_point.column))
                    {
                        return Some(child);
                    }
                }
            }
        }

        None
    }

    // Check if a node is likely a local variable
    fn is_local_variable(&self, node: &Node, tree: &Tree) -> bool {
        if node.kind() != "identifier" {
            return false;
        }

        // Check if the identifier starts with lowercase letter or underscore
        let source_code = tree
            .root_node()
            .utf8_text(self.document.as_bytes())
            .unwrap_or("");
        let node_text = node.utf8_text(source_code.as_bytes()).unwrap_or("");

        if !node_text.starts_with(|c: char| c.is_lowercase() || c == '_') {
            return false;
        }

        // Check if this is a method call rather than a local variable
        // Method calls are usually in a call_method or method context
        let parent = node.parent();
        if let Some(parent_node) = parent {
            let parent_kind = parent_node.kind();

            // If the identifier is part of a call expression and is the method name,
            // then it's a method call, not a variable
            if parent_kind == "call" {
                let method_name = parent_node.child_by_field_name("method");
                if let Some(method_node) = method_name {
                    if method_node.id() == node.id() {
                        return false; // It's a method call, not a variable
                    }
                }
            }

            // Check for method invocation without arguments (e.g., "obj.method")
            if parent_kind == "method_call" || parent_kind == "call_method" {
                return false;
            }

            // Check for attribute accessors in dot notation (e.g., "obj.name")
            if parent_kind == "call" {
                // If we're in a call expression with a dot operator
                // and our node is on the right side of the dot, it's likely a method call
                if let Some(receiver) = parent_node.child_by_field_name("receiver") {
                    if let Some(method) = parent_node.child_by_field_name("method") {
                        if method.id() == node.id() {
                            return false; // It's an attribute/method call
                        }
                    }
                }
            }

            // Check if this is a method parameter
            if parent_kind == "parameters"
                || parent_kind == "optional_parameter"
                || parent_kind == "keyword_parameter"
                || parent_kind == "rest_parameter"
                || parent_kind == "hash_splat_parameter"
            {
                // Find the method that contains these parameters
                let mut current = parent_node.clone();
                while let Some(p) = current.parent() {
                    if p.kind() == "method" || p.kind() == "singleton_method" {
                        return true; // It's a method parameter, which is a kind of local variable
                    }
                    current = p;
                }
            }

            // Check if this is a block parameter
            if parent_kind == "block_parameters" {
                return true; // It's a block parameter, which is a kind of local variable
            }
        }

        // If we got here, it's likely a local variable
        true
    }

    // Determine the fully qualified name when the node is a method call
    fn determine_method_call_fqn(&self, tree: &Tree, node: Node, position: Position) -> String {
        let source_code = self.document.as_str();
        let node_text = &source_code[node.range().start_byte..node.range().end_byte];

        // Get parent node to check context
        if let Some(parent) = node.parent() {
            if parent.kind() == "call" {
                // This is a method call like "obj.method"
                if let Some(receiver) = parent.child_by_field_name("receiver") {
                    // Extract receiver text
                    let receiver_range = receiver.range();
                    let receiver_text =
                        &source_code[receiver_range.start_byte..receiver_range.end_byte];

                    // For attribute accessors, try to determine the receiver type
                    // For now, we just use the receiver name
                    if receiver_text
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_uppercase())
                    {
                        // If receiver starts with uppercase, it's likely a class name
                        return format!("{}#{}", receiver_text, node_text);
                    } else {
                        // For other receivers, try to find their type
                        // For now, we just return the method name
                        return node_text.to_string();
                    }
                }
            }
        }

        // Default to just the method name
        node_text.to_string()
    }

    // Determine the fully qualified name based on context
    pub fn determine_fully_qualified_name(
        &self,
        tree: &Tree,
        node: Node,
        position: Position,
    ) -> String {
        let source_code = self.document.as_str();
        let node_text = &source_code[node.range().start_byte..node.range().end_byte];

        // Get the current context (namespace)
        let current_context = self.find_current_context(tree, position);

        // Get the current method if we're inside one
        let current_method = self.find_current_method(tree, position);

        // Handle different node types
        match node.kind() {
            "identifier" => {
                // Check if this identifier is a method name in a method definition
                if let Some(parent) = node.parent() {
                    if parent.kind() == "method" {
                        // This is likely a method name in a method definition
                        let method_name = node_text.to_string();
                        if !current_context.is_empty() {
                            return format!("{}#{}", current_context, method_name);
                        } else {
                            return method_name;
                        }
                    }

                    // Check if this is a method call in a call expression
                    if parent.kind() == "call" {
                        if let Some(method) = parent.child_by_field_name("method") {
                            if method.id() == node.id() {
                                // This is a method call like obj.method
                                return self.determine_method_call_fqn(tree, node, position);
                            }
                        }
                    }

                    // Check if this is a method parameter
                    if parent.kind() == "parameters"
                        || parent.kind() == "optional_parameter"
                        || parent.kind() == "keyword_parameter"
                        || parent.kind() == "rest_parameter"
                        || parent.kind() == "hash_splat_parameter"
                    {
                        // Find the method that contains these parameters
                        let mut current = parent.clone();
                        let mut method_name = None;

                        while let Some(p) = current.parent() {
                            if p.kind() == "method" || p.kind() == "singleton_method" {
                                if let Some(method_name_node) = p.child_by_field_name("name") {
                                    let method_name_text =
                                        &source_code[method_name_node.range().start_byte
                                            ..method_name_node.range().end_byte];
                                    method_name = Some(method_name_text.to_string());
                                }
                                break;
                            }
                            current = p;
                        }

                        if let Some(method_name) = method_name {
                            // Format for method parameter: namespace#method$param_name
                            if !current_context.is_empty() {
                                return format!(
                                    "{}#{}${}",
                                    current_context, method_name, node_text
                                );
                            } else {
                                return format!("{}${}", method_name, node_text);
                            }
                        }
                    }

                    // Check if this is a block parameter
                    if parent.kind() == "block_parameters" {
                        // Find the method that contains this block
                        let mut current = parent.clone();
                        let mut method_name = None;

                        while let Some(p) = current.parent() {
                            if p.kind() == "method" || p.kind() == "singleton_method" {
                                if let Some(method_name_node) = p.child_by_field_name("name") {
                                    let method_name_text =
                                        &source_code[method_name_node.range().start_byte
                                            ..method_name_node.range().end_byte];
                                    method_name = Some(method_name_text.to_string());
                                }
                                break;
                            }
                            current = p;
                        }

                        // Format for block parameter: namespace#method$block$param_name or $block$param_name
                        if let Some(method_name) = method_name.or_else(|| current_method.clone()) {
                            if !current_context.is_empty() {
                                return format!(
                                    "{}#{}$block${}",
                                    current_context, method_name, node_text
                                );
                            } else {
                                return format!("{}$block${}", method_name, node_text);
                            }
                        } else {
                            if !current_context.is_empty() {
                                return format!("{}#$block${}", current_context, node_text);
                            } else {
                                return format!("$block${}", node_text);
                            }
                        }
                    }
                }

                // Check if this is a usage of a block parameter
                // We need to check if we're inside a block and if the identifier matches a block parameter
                let mut current_node = node.clone();
                let mut in_block = false;
                let mut block_params = Vec::new();

                while let Some(parent) = current_node.parent() {
                    if parent.kind() == "do_block" || parent.kind() == "block" {
                        in_block = true;
                        if let Some(params) = parent.child_by_field_name("parameters") {
                            for i in 0..params.named_child_count() {
                                if let Some(param) = params.named_child(i) {
                                    if param.kind() == "identifier" {
                                        let param_text = &source_code
                                            [param.range().start_byte..param.range().end_byte];
                                        block_params.push(param_text.to_string());
                                    }
                                }
                            }
                        }
                        break;
                    }
                    current_node = parent;
                }

                if in_block && block_params.contains(&node_text.to_string()) {
                    // This is a usage of a block parameter
                    if let Some(method_name) = current_method {
                        if !current_context.is_empty() {
                            return format!(
                                "{}#{}$block${}",
                                current_context, method_name, node_text
                            );
                        } else {
                            return format!("{}$block${}", method_name, node_text);
                        }
                    } else {
                        return format!("$block${}", node_text);
                    }
                }

                // This could be a local variable or a method call
                // Check if node is used as a local variable
                if self.is_local_variable(&node, tree) {
                    // Format for local variable: namespace#method$varname or $varname
                    if let Some(method_name) = current_method {
                        if current_context.is_empty() {
                            format!("{}#${}", method_name, node_text)
                        } else {
                            format!("{}#{}${}", current_context, method_name, node_text)
                        }
                    } else {
                        if current_context.is_empty() {
                            format!("${}", node_text)
                        } else {
                            format!("{}#${}", current_context, node_text)
                        }
                    }
                } else {
                    // It's a method call - check if it's part of a call expression
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "call" {
                            // Check if we can determine the receiver type
                            if let Some(receiver) = parent.child_by_field_name("receiver") {
                                // Try to determine the type of the receiver
                                // For now, just use the receiver's text as the class context
                                let receiver_range = receiver.range();
                                let receiver_text = &source_code
                                    [receiver_range.start_byte..receiver_range.end_byte];

                                // If the receiver is an identifier that starts with a capital letter,
                                // it's likely a class/module, so use it as context
                                if receiver_text
                                    .chars()
                                    .next()
                                    .map_or(false, |c| c.is_uppercase())
                                {
                                    return format!("{}#{}", receiver_text, node_text);
                                }
                            }
                        }
                    }

                    // Default method call formatting with current context
                    if !current_context.is_empty() {
                        format!("{}#{}", current_context, node_text)
                    } else {
                        node_text.to_string()
                    }
                }
            }
            "instance_variable" => {
                // Format for instance variable: namespace#@varname
                if !current_context.is_empty() {
                    format!("{}#{}", current_context, node_text)
                } else {
                    node_text.to_string()
                }
            }
            "constant" => {
                // Format for constant: namespace::CONSTNAME
                if !current_context.is_empty() {
                    format!("{}::{}", current_context, node_text)
                } else {
                    node_text.to_string()
                }
            }
            _ => node_text.to_string(), // Default for other node types
        }
    }

    // Find the current method at position
    pub fn find_current_method(&self, tree: &Tree, position: Position) -> Option<String> {
        let point = Point::new(position.line as usize, position.character as usize);
        let mut cursor = tree.root_node().walk();

        // Find the node at the position
        if let Some(node) = self.find_node_at_point(&mut cursor, point) {
            // Walk up the tree to find containing method node
            let mut current = node;
            while let Some(parent) = current.parent() {
                if parent.kind() == "method" || parent.kind() == "singleton_method" {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        return Some(self.get_node_text(name_node));
                    }
                }
                current = parent;
            }
        }

        None
    }

    // Find the current context (class/module) at position
    pub fn find_current_context(&self, tree: &Tree, position: Position) -> String {
        let point = Point::new(position.line as usize, position.character as usize);
        let mut cursor = tree.root_node().walk();
        let mut namespace_stack = Vec::new();

        // Find the node at the position
        if let Some(node) = self.find_node_at_point(&mut cursor, point) {
            // Walk up the tree to find containing class/module nodes
            let mut current = node;
            while let Some(parent) = current.parent() {
                if parent.kind() == "class" || parent.kind() == "module" {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        let name = self.get_node_text(name_node);
                        namespace_stack.push(name);
                    }
                }
                current = parent;
            }
        }

        // Reverse the stack to get the correct namespace order (outermost to innermost)
        namespace_stack.reverse();

        // Join with :: to form the fully qualified namespace
        namespace_stack.join("::")
    }

    // Helper to get node text
    fn get_node_text(&self, node: Node) -> String {
        let source_code = self.document.as_str();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        if start_byte <= end_byte && end_byte <= source_code.len() {
            source_code[start_byte..end_byte].to_string()
        } else {
            String::new()
        }
    }
}

// Helper functions for working with tree-sitter and LSP positions

// Get a vector of starting byte offsets for each line
pub fn get_line_starts(text: &[u8]) -> Vec<usize> {
    let mut line_starts = vec![0];
    let mut offset = 0;

    for byte in text {
        offset += 1;
        if *byte == b'\n' {
            line_starts.push(offset);
        }
    }

    line_starts
}

// Convert an LSP position to a byte offset
pub fn position_to_offset(position: Position, line_starts: &[usize]) -> Option<usize> {
    let line_index = position.line as usize;
    if line_index >= line_starts.len() {
        return None;
    }

    let line_start = line_starts[line_index];
    Some(line_start + position.character as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    // Helper to create a test document and get a tree from it
    fn setup_test(code: &str) -> (RubyAnalyzer, Tree) {
        let mut analyzer = RubyAnalyzer::new();
        analyzer.document = code.to_string();

        let mut parser = Parser::new();
        parser.set_language(tree_sitter_ruby::language()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        (analyzer, tree)
    }

    // Test that node text is correctly extracted
    #[test]
    fn test_node_text_extraction() {
        let code = "x = 10\ny = 20\nz = x + y";
        let (_analyzer, tree) = setup_test(code);

        // Get the first variable node (x)
        let root = tree.root_node();
        let assignment_node = root.child(0).unwrap();
        let left_node = assignment_node.child_by_field_name("left").unwrap();

        // Verify text is extracted correctly
        let node_text = left_node.utf8_text(code.as_bytes()).unwrap();
        assert_eq!(node_text, "x");
    }

    // Test for the specific issue with parentheses in identifiers
    #[test]
    fn test_no_parentheses_in_identifiers() {
        let code = "age_threshold = 18\nage >= age_threshold";
        let mut analyzer = RubyAnalyzer::new();

        // Position points to "age_threshold" in the second line
        let position = Position {
            line: 1,
            character: 7, // Points to "age_threshold" in the comparison
        };

        let fqn = analyzer.find_identifier_at_position(code, position);
        assert!(fqn.is_some());
        let identifier = fqn.unwrap();

        // Make sure there are no parentheses in the result
        assert!(!identifier.contains("("));
        assert!(!identifier.contains(")"));

        // Make sure it correctly identified "age_threshold"
        assert!(identifier.contains("age_threshold"));
    }

    // Test for the issue with the "$fier))" that was in the logs
    #[test]
    fn test_identifier_extraction_no_sexp_artifacts() {
        let code = "person = Person.new(\"John\", 30)";
        let mut analyzer = RubyAnalyzer::new();

        // Position points to "Person" in "Person.new"
        let position = Position {
            line: 0,
            character: 10, // Points to "Person" in Person.new
        };

        let fqn = analyzer.find_identifier_at_position(code, position);
        assert!(fqn.is_some());
        let identifier = fqn.unwrap();

        // Should contain exactly "Person" (possibly with namespace)
        assert!(identifier == "Person" || identifier.ends_with("::Person"));

        // Should not contain any artifacts from S-expressions
        assert!(!identifier.contains("("));
        assert!(!identifier.contains(")"));
        assert!(!identifier.contains("$fier"));
    }

    // Test real-world class patterns similar to what's in test.rb
    #[test]
    fn test_realistic_ruby_class() {
        let code = r#"class Person
  attr_accessor :name, :age

  def initialize(name, age)
    @name = name  # instance variable
    @age = age    # instance variable
  end

  def greet
    greeting = "Hello, my name is #{@name}"  # local variable
    puts greeting
  end

  def birthday
    @age += 1
    puts "Happy Birthday! Now I am #{@age} years old."
  end
end

# Create a new person
person = Person.new("John", 30)
person.greet
person.birthday"#;
        let mut analyzer = RubyAnalyzer::new();

        // Test finding the greeting local variable
        let greeting_position = Position {
            line: 10,
            character: 9, // Points to "greeting" in "puts greeting"
        };

        let greeting_fqn = analyzer.find_identifier_at_position(code, greeting_position);
        assert!(greeting_fqn.is_some());
        let greeting_id = greeting_fqn.unwrap();

        // Should be "Person#greet$greeting" with the new implementation
        assert_eq!(greeting_id, "Person#greet$greeting");

        // Test finding the person local variable
        let person_position = Position {
            line: 21,
            character: 0, // Points to "person" in "person.greet"
        };

        let person_fqn = analyzer.find_identifier_at_position(code, person_position);
        assert!(person_fqn.is_some());
        let person_id = person_fqn.unwrap();

        // Should be "$person" (a top-level local variable)
        assert_eq!(person_id, "$person");
    }

    // Test to verify method calls are correctly identified as methods, not local variables
    #[test]
    fn test_method_call_identification() {
        let code = r#"
class Person
  def greet
    puts "Hello"
  end
end

person = Person.new
person.greet  # Method call, not a local variable
"#;
        let mut analyzer = RubyAnalyzer::new();
        analyzer.document = code.to_string();

        // Try multiple positions to find the correct one
        let positions = [
            (8, 7),  // After dot
            (8, 8),  // At 'g'
            (8, 9),  // At 'r'
            (8, 10), // At 'e'
        ];

        for (line, character) in positions {
            println!("Testing position ({}, {})", line, character);
            let method_position = Position { line, character };

            // Parse the tree
            let tree = analyzer.parser.parse(code, None).unwrap();

            // Find the node at the position
            let point = Point::new(line as usize, character as usize);
            let mut cursor = tree.root_node().walk();

            if let Some(node) = analyzer.find_node_at_point(&mut cursor, point) {
                // Print node information for debugging
                let node_kind = node.kind();
                let node_text = node.utf8_text(code.as_bytes()).unwrap_or("");
                println!(
                    "Found node of kind '{}' with text '{}'",
                    node_kind, node_text
                );

                // Get fully qualified name
                let fqn = analyzer.determine_fully_qualified_name(&tree, node, method_position);
                println!("FQN: {}", fqn);

                if node_kind == "identifier" && node_text == "greet" {
                    // This is the node we're looking for
                    assert!(
                        !fqn.contains("$"),
                        "Method was incorrectly identified as a local variable"
                    );
                    assert!(
                        fqn == "greet" || fqn.ends_with("#greet"),
                        "Method was not identified correctly. Got: '{}'",
                        fqn
                    );
                    return; // Test passes if we find the correct node
                }
            } else {
                println!("No node found at position");
            }
        }

        // If we get here, we didn't find the right node
        panic!("Couldn't find 'greet' identifier node in any tested position");
    }

    // Test for the specific case in test.rb with local variables in methods
    #[test]
    fn test_analyzer_with_test_rb_scenario() {
        let code = r#"
class User
  def initialize(name, age)
    @name = name
    @age = age
  end

  def adult?
    age_threshold = 18  # Local variable definition
    @age >= age_threshold  # Local variable reference
  end

  def greet
    greeting = "Hello, #{@name}!"  # Local variable definition
    puts greeting  # Local variable reference
    greeting  # Another reference to the same variable
  end
end

user = User.new("John", 25)
puts user.adult?
puts user.greet
"#;
        let mut analyzer = RubyAnalyzer::new();

        // Test local variable age_threshold
        let age_threshold_pos = Position {
            line: 9,       // Line with @age >= age_threshold
            character: 13, // Position of age_threshold in the comparison
        };

        let age_threshold_fqn = analyzer.find_identifier_at_position(code, age_threshold_pos);
        assert!(
            age_threshold_fqn.is_some(),
            "Should find age_threshold identifier"
        );
        let fqn = age_threshold_fqn.unwrap();

        // It should be identified as a local variable (has $ prefix)
        assert!(
            fqn.contains("$"),
            "age_threshold should be identified as a local variable"
        );
        assert!(
            fqn.contains("age_threshold"),
            "FQN should contain the variable name"
        );

        // Test method call adult?
        let adult_method_pos = Position {
            line: 20,      // Line with 'puts user.adult?'
            character: 12, // Position of adult? in the method call (verified)
        };

        let adult_method_fqn = analyzer.find_identifier_at_position(code, adult_method_pos);
        assert!(
            adult_method_fqn.is_some(),
            "Should find adult? method identifier"
        );
        let fqn = adult_method_fqn.unwrap();

        // It should be identified as a method call (no $ prefix)
        assert!(
            !fqn.contains("$"),
            "adult? should be identified as a method, not a local variable"
        );
        assert!(
            fqn.contains("adult?") || fqn == "adult?",
            "FQN should contain the method name"
        );

        // Test greeting local variable in puts statement
        let greeting_pos = Position {
            line: 14,     // Line with "puts greeting"
            character: 9, // Position of greeting in the puts statement
        };

        let greeting_fqn = analyzer.find_identifier_at_position(code, greeting_pos);
        assert!(greeting_fqn.is_some(), "Should find greeting identifier");
        let fqn = greeting_fqn.unwrap();

        // It should be identified as a local variable (has $ prefix)
        assert!(
            fqn.contains("$"),
            "greeting should be identified as a local variable"
        );
        assert!(
            fqn.contains("greeting"),
            "FQN should contain the variable name"
        );
    }

    // Test for the specific instance variable and method name positions in test.rb
    #[test]
    fn test_analyzer_instance_vars_and_methods() {
        let code = r#"
class Person
  attr_accessor :name, :age

  def initialize(name, age)
    @name = name  # instance variable
    @age = age    # instance variable
  end

  def greet
    greeting = "Hello, my name is #{@name}"  # local variable
    puts greeting
  end

  def birthday
    @age += 1
    puts "Happy Birthday! Now I am #{@age} years old."
  end
end
"#;
        let mut analyzer = RubyAnalyzer::new();
        analyzer.document = code.to_string();

        let tree = analyzer.parser.parse(code, None).unwrap();
        let source_code = code;

        println!("\nAnalyzing nodes in test.rb positions:");

        // Problematic positions from user logs
        let positions = [
            (4, 7), // Around 'initialize' method name
            (5, 7), // Around '@name' instance variable
        ];

        for (line, character) in positions {
            println!("\nChecking position ({}, {})", line, character);

            // Get the actual line content for reference
            if let Some(line_content) = code.lines().nth(line) {
                println!("Line content: '{}'", line_content);
            }

            // Find the node at the position
            let point = Point::new(line as usize, character as usize);
            let mut cursor = tree.root_node().walk();

            if let Some(node) = analyzer.find_node_at_point(&mut cursor, point) {
                let node_kind = node.kind();
                let range = node.range();
                let node_text = &source_code[range.start_byte..range.end_byte];

                println!("Found node: kind='{}', text='{}'", node_kind, node_text);

                // Check parent node type
                if let Some(parent) = node.parent() {
                    let parent_kind = parent.kind();
                    let parent_range = parent.range();
                    let parent_text = &source_code[parent_range.start_byte..parent_range.end_byte];
                    println!(
                        "Parent node: kind='{}', text='{}'",
                        parent_kind, parent_text
                    );
                }

                // Try to get the fully qualified name
                let pos = Position {
                    line: line as u32,
                    character: character as u32,
                };
                let fqn = analyzer.determine_fully_qualified_name(&tree, node, pos);
                println!("FQN result: '{}'", fqn);
            } else {
                println!("No node found at this position");
            }
        }
    }

    #[test]
    fn test_method_and_block_parameters() {
        let code = r#"
class Person
  def initialize(name, age = 30)
    @name = name
    @age = age
  end

  def greet
    yield @name if block_given?
  end
end

person = Person.new("John")
person.greet do |name|
  puts "Hello, #{name}!"
end
"#;
        let (mut analyzer, tree) = setup_test(code);

        // Test method parameter 'name' in initialize
        let position = Position::new(2, 20); // Position at 'name' parameter
        let identifier = analyzer.find_identifier_at_position(code, position);
        assert!(
            identifier.is_some(),
            "Should find identifier at method parameter position"
        );

        let name_id = identifier.unwrap();
        assert!(
            name_id.contains("initialize$name"),
            "Method parameter should have correct FQN, got: {}",
            name_id
        );

        // Test method parameter 'age' in initialize
        let position = Position::new(2, 26); // Position at 'age' parameter
        let identifier = analyzer.find_identifier_at_position(code, position);
        assert!(
            identifier.is_some(),
            "Should find identifier at method parameter position"
        );

        let age_id = identifier.unwrap();
        assert!(
            age_id.contains("initialize$age"),
            "Method parameter should have correct FQN, got: {}",
            age_id
        );

        // Test block parameter 'name'
        let position = Position::new(13, 18); // Position at block parameter 'name', not 'do'
        let identifier = analyzer.find_identifier_at_position(code, position);

        assert!(
            identifier.is_some(),
            "Should find identifier at block parameter position"
        );

        let block_param_id = identifier.unwrap();
        assert!(
            block_param_id.contains("$block$name"),
            "Block parameter should have correct FQN, got: {}",
            block_param_id
        );

        // Test usage of method parameter inside method body
        let position = Position::new(3, 13); // Position at 'name' usage in initialize method
        let identifier = analyzer.find_identifier_at_position(code, position);
        assert!(
            identifier.is_some(),
            "Should find identifier at method parameter usage"
        );

        let name_usage_id = identifier.unwrap();
        assert!(
            name_usage_id.contains("initialize$name"),
            "Method parameter usage should have correct FQN, got: {}",
            name_usage_id
        );

        // Test usage of block parameter inside block body
        let position = Position::new(14, 20); // Position at 'name' usage in block
        let identifier = analyzer.find_identifier_at_position(code, position);

        assert!(
            identifier.is_some(),
            "Should find identifier at block parameter usage"
        );

        let block_param_usage_id = identifier.unwrap();
        assert!(
            block_param_usage_id.contains("$block$name"),
            "Block parameter usage should have correct FQN, got: {}",
            block_param_usage_id
        );
    }
}
