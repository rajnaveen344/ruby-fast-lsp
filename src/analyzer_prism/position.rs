use lsp_types::{Position, Range};
use ruby_prism::Location;

/// Convert an LSP position to a Prism location
/// LSP positions are 0-based, while Prism locations are byte offsets
pub fn lsp_pos_to_prism_loc(pos: Position, content: &str) -> usize {
    let mut byte_offset = 0;
    let mut line_count = 0;
    let mut char_count = 0;

    for c in content.chars() {
        if line_count == pos.line && char_count == pos.character {
            break;
        }

        if c == '\n' {
            line_count += 1;
            char_count = 0;
        } else {
            char_count += 1;
        }

        byte_offset += c.len_utf8();
    }

    byte_offset
}

/// Convert a Prism location back to an LSP position
pub fn prism_loc_to_lsp_pos(loc: Location, content: &str) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for c in content.chars() {
        if current_offset >= loc.start_offset() {
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
    todo!()
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
        let body_node = def_node
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

        let body_loc = body_node.location();
        let body_start_offset = body_loc.start_offset();
        // "puts" starts after "def foo\n    " which is 12 bytes
        assert_eq!(body_start_offset, 12);

        // Test converting from LSP position to Prism location for body
        let pos_for_body = Position::new(1, 4); // Line 1, Column 4 (after indentation)
        assert_eq!(
            lsp_pos_to_prism_loc(pos_for_body, content),
            body_start_offset
        );

        // Test the reverse mapping: Prism location to LSP position
        let name_pos = prism_loc_to_lsp_pos(name_loc, content);
        assert_eq!(name_pos, Position::new(0, 4)); // "foo" starts at line 0, column 4

        let body_pos = prism_loc_to_lsp_pos(body_loc, content);
        assert_eq!(body_pos, Position::new(1, 4)); // "puts" starts at line 1, column 4

        // Test edge cases
        assert_eq!(lsp_pos_to_prism_loc(Position::new(0, 0), content), 0); // Start of file
        assert_eq!(
            lsp_pos_to_prism_loc(Position::new(10, 10), content),
            content.len()
        ); // Past end of file
        assert_eq!(lsp_pos_to_prism_loc(Position::new(0, 0), ""), 0); // Empty string
    }

    #[test]
    fn test_prism_loc_to_lsp_pos() {
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
            .expect("Module body") // Get Option<Node>
            .as_statements_node()
            .expect("Should be StatementsNode")
            .body()
            .iter()
            .nth(0)
            .expect("Node not found at index")
            .as_constant_write_node()
            .expect("Should find ConstantWriteNode");

        // Test with location of 'module Foo' keyword (0..6)
        let module_loc = module.location();
        assert_eq!(
            prism_loc_to_lsp_pos(module_loc, content),
            Position::new(0, 0)
        ); // Start of 'module'

        // Test with location of 'CONST' (11..16)
        let const_loc = const_assign.name_loc();
        assert_eq!(
            prism_loc_to_lsp_pos(const_loc, content),
            Position::new(1, 2)
        ); // Start of 'CONST'

        // Test with location of entire ConstantWriteNode 'CONST = 1' (11..20)
        let const_assign_loc = const_assign.location();
        assert_eq!(
            prism_loc_to_lsp_pos(const_assign_loc, content),
            Position::new(1, 2)
        ); // Start of 'CONST'
    }
}
