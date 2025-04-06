use lsp_types::{Position, Range};
use ruby_prism::Location;

/// Convert an LSP position to a Prism location (byte offset)
/// LSP positions are 0-based, while Prism locations are byte offsets
pub fn lsp_pos_to_prism_loc(pos: Position, content: &str) -> usize {
    let mut byte_offset = 0;
    let mut line_count = 0;
    let mut char_count = 0;

    for c in content.chars() {
        if line_count > pos.line || (line_count == pos.line && char_count >= pos.character) {
            break;
        }

        if c == '\n' {
            line_count += 1;
            char_count = 0;
        } else {
            // Handle UTF-8 multi-byte characters correctly if necessary, though char_count is sufficient for LSP
            char_count += 1;
        }

        byte_offset += c.len_utf8();
    }

    // If the position is beyond the content length, clamp it to the end
    std::cmp::min(byte_offset, content.len())
}

/// Convert a Prism byte offset back to an LSP position (0-based line and character)
pub fn prism_offset_to_lsp_pos(offset: usize, content: &str) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for c in content.chars() {
        if current_offset >= offset {
            break;
        }

        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }

        current_offset += c.len_utf8();
    }

    Position::new(line as u32, character as u32)
}

pub fn prism_loc_to_lsp_range(loc: Location, content: &str) -> Range {
    let start_pos = prism_offset_to_lsp_pos(loc.start_offset(), content);
    let end_pos = prism_offset_to_lsp_pos(loc.end_offset(), content);
    Range::new(start_pos, end_pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use ruby_prism::parse;

    #[test]
    fn test_lsp_pos_to_prism_loc() {
        let content = "def foo\n    puts 'Hello'\nend";

        // Parse the content to get real nodes with locations
        let result = parse(content.as_bytes());
        let program = result
            .node()
            .as_program_node()
            .expect("Should be a program node");

        // Get method definition node (DefNode)
        let def_node = program
            .statements()
            .body()
            .iter()
            .nth(0)
            .expect("Should have a node")
            .as_def_node()
            .expect("Should be a def node");

        // Get the location of the method name "foo"
        let name_loc = def_node.name_loc();
        let name_start_offset = name_loc.start_offset();
        // "foo" starts at the 4th byte position (after "def ")
        assert_eq!(name_start_offset, 4);

        // Test converting from LSP position to Prism location
        let pos_for_name = Position::new(0, 4); // Line 0, Column 4 should be the start of "foo"
        assert_eq!(
            lsp_pos_to_prism_loc(pos_for_name, content),
            name_start_offset
        );

        // Get location of the method body content "puts 'Hello'"
        let body_call_node = def_node // Renamed for clarity
            .body()
            .expect("Method should have a body")
            .as_statements_node()
            .expect("Body should be statements")
            .body()
            .iter()
            .nth(0)
            .expect("Should have a statement")
            .as_call_node()
            .expect("Should be a call node");

        let body_loc = body_call_node.location();
        let body_start_offset = body_loc.start_offset();
        // "puts" starts after "def foo\n    " which is 12 bytes
        assert_eq!(body_start_offset, 12);

        // Test converting from LSP position to Prism location for body
        let pos_for_body = Position::new(1, 4); // Line 1, Column 4 (after indentation)
        assert_eq!(
            lsp_pos_to_prism_loc(pos_for_body, content),
            body_start_offset
        );

        // Test the reverse mapping: Prism location to LSP position using the new helper
        let name_pos = prism_offset_to_lsp_pos(name_start_offset, content);
        assert_eq!(name_pos, Position::new(0, 4)); // "foo" starts at line 0, column 4

        let body_pos = prism_offset_to_lsp_pos(body_start_offset, content);
        assert_eq!(body_pos, Position::new(1, 4)); // "puts" starts at line 1, column 4

        // Test edge cases
        assert_eq!(lsp_pos_to_prism_loc(Position::new(0, 0), content), 0); // Start of file
                                                                           // Test position past end of content - should clamp to content length
        assert_eq!(
            lsp_pos_to_prism_loc(Position::new(10, 10), content),
            content.len()
        );
        assert_eq!(lsp_pos_to_prism_loc(Position::new(0, 0), ""), 0); // Empty string
        assert_eq!(lsp_pos_to_prism_loc(Position::new(0, 3), "abc"), 3);
        assert_eq!(lsp_pos_to_prism_loc(Position::new(0, 4), "abc"), 3); // Clamp past end
    }

    #[test]
    fn test_prism_offset_to_lsp_pos() {
        let content = "module Foo\n  CONST = 1\nend";
        // Parse the code to get valid Location objects
        let result = parse(content.as_bytes());
        let ast = result.node();
        let program = ast.as_program_node().expect("Should be ProgramNode");
        let module = program
            .statements()
            .body()
            .iter()
            .nth(0)
            .expect("Node not found at index")
            .as_module_node()
            .expect("Should find ModuleNode");
        let const_assign = module
            .body()
            .expect("Module body")
            .as_statements_node()
            .expect("Should be StatementsNode")
            .body()
            .iter()
            .nth(0)
            .expect("Node not found at index")
            .as_constant_write_node()
            .expect("Should find ConstantWriteNode");

        // Test with location of 'module Foo' keyword (0..6)
        let module_start_offset = module.location().start_offset();
        assert_eq!(
            prism_offset_to_lsp_pos(module_start_offset, content),
            Position::new(0, 0)
        ); // Start of 'module'

        // Test with location of 'CONST' (11..16)
        let const_name_start_offset = const_assign.name_loc().start_offset();
        assert_eq!(
            prism_offset_to_lsp_pos(const_name_start_offset, content),
            Position::new(1, 2)
        ); // Start of 'CONST'

        // Test with location of entire ConstantWriteNode 'CONST = 1' (11..20)
        let const_assign_start_offset = const_assign.location().start_offset();
        assert_eq!(
            prism_offset_to_lsp_pos(const_assign_start_offset, content),
            Position::new(1, 2)
        ); // Start of 'CONST'

        // Test end of file
        let end_offset = content.len();
        assert_eq!(
            prism_offset_to_lsp_pos(end_offset, content),
            Position::new(2, 3)
        ); // Position after 'end'
        assert_eq!(
            prism_offset_to_lsp_pos(end_offset + 5, content),
            Position::new(2, 3)
        ); // Past end should clamp

        // Test empty string
        assert_eq!(prism_offset_to_lsp_pos(0, ""), Position::new(0, 0));
        assert_eq!(prism_offset_to_lsp_pos(5, ""), Position::new(0, 0));
    }

    #[test]
    fn test_prism_loc_to_lsp_range() {
        let content =
            "class MyClass\n  def my_method # Comment\n    123 # Another comment\n  end\nend";
        let result = parse(content.as_bytes());
        let ast = result.node();
        let program = ast.as_program_node().expect("ProgramNode");
        let class = program
            .statements()
            .body()
            .iter()
            .nth(0)
            .expect("Class node not found")
            .as_class_node()
            .expect("ClassNode");
        let def = class
            .body()
            .unwrap()
            .as_statements_node()
            .unwrap()
            .body()
            .iter()
            .nth(0)
            .expect("Def node not found")
            .as_def_node()
            .expect("DefNode");

        // 1. Test range for the entire class definition: 'class MyClass..end'
        // Location: start=0, end=59
        let class_loc = class.location();
        let expected_class_range = Range::new(Position::new(0, 0), Position::new(4, 3)); // Start of 'class', end of 'end'
        assert_eq!(
            prism_loc_to_lsp_range(class_loc, content),
            expected_class_range
        );

        // 2. Test range for the method definition: 'def my_method..end'
        // Location: start=16, end=55
        let def_loc = def.location();
        let expected_def_range = Range::new(Position::new(1, 2), Position::new(3, 5)); // Start of 'def', end of 'end'
        assert_eq!(prism_loc_to_lsp_range(def_loc, content), expected_def_range);

        // 3. Test range for the method name: 'my_method'
        // Location: start=20, end=29
        let def_name_loc = def.name_loc();
        let expected_def_name_range = Range::new(Position::new(1, 6), Position::new(1, 15)); // Start/end of 'my_method'
        assert_eq!(
            prism_loc_to_lsp_range(def_name_loc, content),
            expected_def_name_range
        );

        // 4. Test range for the integer literal '123' inside the method
        // Location: start=41, end=44
        let int_node = def
            .body()
            .unwrap()
            .as_statements_node()
            .unwrap()
            .body()
            .iter()
            .nth(0)
            .expect("Int node not found")
            .as_integer_node()
            .expect("IntegerNode");
        let int_loc = int_node.location();
        let expected_int_range = Range::new(Position::new(2, 4), Position::new(2, 7)); // Start/end of '123'
        assert_eq!(prism_loc_to_lsp_range(int_loc, content), expected_int_range);

        // 6. Test location spanning multiple lines
        let multi_line_content = "foo = 1 +\n  2";
        let multi_result = parse(multi_line_content.as_bytes());
        let multi_program = multi_result.node().as_program_node().expect("ProgramNode");
        let call_node = multi_program
            .statements()
            .body()
            .iter()
            .nth(0)
            .expect("Write node not found")
            .as_local_variable_write_node()
            .unwrap()
            .value()
            .as_call_node()
            .unwrap();
        let call_loc = call_node.location(); // Should cover "1 +\n  2"
        let expected_multi_range = Range::new(Position::new(0, 6), Position::new(1, 3)); // Start '1', end '2'
        assert_eq!(
            prism_loc_to_lsp_range(call_loc, multi_line_content),
            expected_multi_range
        );
    }
}
