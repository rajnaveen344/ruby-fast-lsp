use crate::server::RubyLanguageServer;
use ruby_prism::{parse, Visit};
use tower_lsp::lsp_types::{
    DocumentOnTypeFormattingOptions, DocumentOnTypeFormattingParams, Range, TextEdit,
};

/// Visitor to detect if we have a conditional assignment pattern
struct ConditionalAssignmentVisitor {
    has_conditional_assignment: bool,
    has_standalone_conditional: bool,
}

impl ConditionalAssignmentVisitor {
    fn new() -> Self {
        Self {
            has_conditional_assignment: false,
            has_standalone_conditional: false,
        }
    }
}

impl<'a> Visit<'a> for ConditionalAssignmentVisitor {
    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'a>) {
        // Check if the value being assigned is a conditional
        let value = node.value();
        match &value {
            ruby_prism::Node::IfNode { .. }
            | ruby_prism::Node::UnlessNode { .. }
            | ruby_prism::Node::CaseNode { .. }
            | ruby_prism::Node::WhileNode { .. }
            | ruby_prism::Node::UntilNode { .. } => {
                self.has_conditional_assignment = true;
            }
            _ => {}
        }
        ruby_prism::visit_local_variable_write_node(self, node);
    }

    fn visit_if_node(&mut self, node: &ruby_prism::IfNode<'a>) {
        // This is a standalone if (not part of an assignment)
        if !self.has_conditional_assignment {
            self.has_standalone_conditional = true;
        }
        ruby_prism::visit_if_node(self, node);
    }

    fn visit_unless_node(&mut self, node: &ruby_prism::UnlessNode<'a>) {
        if !self.has_conditional_assignment {
            self.has_standalone_conditional = true;
        }
        ruby_prism::visit_unless_node(self, node);
    }

    fn visit_case_node(&mut self, node: &ruby_prism::CaseNode<'a>) {
        if !self.has_conditional_assignment {
            self.has_standalone_conditional = true;
        }
        ruby_prism::visit_case_node(self, node);
    }

    fn visit_while_node(&mut self, node: &ruby_prism::WhileNode<'a>) {
        if !self.has_conditional_assignment {
            self.has_standalone_conditional = true;
        }
        ruby_prism::visit_while_node(self, node);
    }

    fn visit_until_node(&mut self, node: &ruby_prism::UntilNode<'a>) {
        if !self.has_conditional_assignment {
            self.has_standalone_conditional = true;
        }
        ruby_prism::visit_until_node(self, node);
    }
}

/// Uses AST analysis to determine if we should add an 'end' keyword
/// This is more robust than string matching for incomplete code
pub fn should_add_end_ast(content: &str) -> bool {
    let result = parse(content.as_bytes());

    let mut visitor = ConditionalAssignmentVisitor::new();
    visitor.visit(&result.node());

    // Add 'end' for standalone conditionals, but NOT for conditional assignments
    let should_add = visitor.has_standalone_conditional && !visitor.has_conditional_assignment;

    should_add
}

pub fn get_document_on_type_formatting_options() -> DocumentOnTypeFormattingOptions {
    DocumentOnTypeFormattingOptions {
        first_trigger_character: "\n".to_string(),
        more_trigger_character: None,
    }
}

pub async fn handle_document_on_type_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentOnTypeFormattingParams,
) -> Option<Vec<TextEdit>> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let trigger_character = &params.ch;

    // Only handle newline character
    if trigger_character != "\n" {
        return None;
    }

    let docs = lang_server.docs.lock();
    let doc_arc = docs.get(uri)?;
    let doc = doc_arc.read();
    let content = &doc.content;

    // Get the line before the current position (where Enter was pressed)
    let line_before = if position.line > 0 {
        position.line - 1
    } else {
        return None;
    };

    let lines: Vec<&str> = content.lines().collect();
    if line_before as usize >= lines.len() {
        return None;
    }

    let previous_line = lines[line_before as usize];
    let trimmed_line = previous_line.trim();

    // Check if the previous line contains Ruby block keywords that need 'end'
    let should_add_end_string = should_add_end_keyword(trimmed_line);

    // Also check with AST analysis on the specific line
    let should_add_end_ast_line = should_add_end_ast(trimmed_line);

    // Check AST analysis on full content for context
    let should_add_end_ast_full = should_add_end_ast(content);

    let should_add_end =
        should_add_end_string || should_add_end_ast_line || should_add_end_ast_full;

    if should_add_end {
        // Calculate indentation from the previous line
        let indentation = get_indentation(previous_line);

        // Create the text edit to insert 'end' with proper indentation
        let end_text = format!("\n{}end", indentation);

        // Insert at the current position (after the newline)
        let edit = TextEdit {
            range: Range {
                start: position,
                end: position,
            },
            new_text: end_text,
        };

        return Some(vec![edit]);
    }

    None
}

pub fn should_add_end_keyword(line: &str) -> bool {
    let trimmed = line.trim();

    // Remove comments
    let line_without_comment = if let Some(pos) = trimmed.find('#') {
        trimmed[..pos].trim()
    } else {
        trimmed
    };

    if line_without_comment.is_empty() {
        return false;
    }

    // Check for specific patterns that should NOT add end

    // 1. Return statements with conditionals
    if line_without_comment.starts_with("return if")
        || line_without_comment.starts_with("return unless")
    {
        return false;
    }

    // 2. Assignment statements with conditionals (e.g., "a = if cond", "result = case x")
    // Use more robust pattern matching to handle variable spacing
    if is_assignment_with_conditional(line_without_comment) {
        return false;
    }

    // Check for block keywords that need 'end' (only when they start the statement)
    if line_without_comment.starts_with("if ")
        || line_without_comment.starts_with("unless ")
        || line_without_comment.starts_with("while ")
        || line_without_comment.starts_with("until ")
    {
        // Make sure it's not a one-liner (doesn't already end with something)
        return !line_without_comment.ends_with("end") && !line_without_comment.contains(" then ");
    }

    // Check for other conditionals in the middle of the line (but not assignments)
    if !line_without_comment.contains(" = ")
        && (line_without_comment.contains(" if ")
            || line_without_comment.contains(" unless ")
            || line_without_comment.contains(" while ")
            || line_without_comment.contains(" until "))
    {
        // Make sure it's not a one-liner
        return !line_without_comment.ends_with("end") && !line_without_comment.contains(" then ");
    }

    // Check for method definitions
    if line_without_comment.starts_with("def ") {
        return true;
    }

    // Check for class/module definitions
    if line_without_comment.starts_with("class ") || line_without_comment.starts_with("module ") {
        return true;
    }

    // Check for case statements
    if line_without_comment.starts_with("case ") {
        return true;
    }

    // Check for begin blocks
    if line_without_comment == "begin" {
        return true;
    }

    // Check for do blocks
    if line_without_comment.ends_with(" do")
        || line_without_comment.ends_with(" do |")
        || line_without_comment.contains(" do |") && line_without_comment.ends_with("|")
    {
        return true;
    }

    // Check for for loops
    if line_without_comment.starts_with("for ") && line_without_comment.contains(" in ") {
        return true;
    }

    false
}

fn is_assignment_with_conditional(line: &str) -> bool {
    // Check for assignment patterns with conditionals, handling variable spacing
    // Patterns like: "a = if", "result  =  case", "x=unless", etc.

    let keywords = ["if", "unless", "case", "while", "until"];

    for keyword in &keywords {
        // Look for pattern: [identifier] [spaces] = [spaces] keyword [space]
        if let Some(eq_pos) = line.find('=') {
            let before_eq = line[..eq_pos].trim();
            let after_eq = line[eq_pos + 1..].trim();

            // Check if there's a valid identifier before =
            if !before_eq.is_empty() && is_valid_identifier(before_eq) {
                // Check if after = starts with our keyword followed by space or end of line
                if after_eq.starts_with(keyword) {
                    let after_keyword = &after_eq[keyword.len()..];
                    if after_keyword.is_empty() || after_keyword.starts_with(' ') {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn is_valid_identifier(s: &str) -> bool {
    // Simple check for valid Ruby identifier (variable name)
    // Should start with letter or underscore, followed by letters, digits, or underscores
    if s.is_empty() {
        return false;
    }

    let chars: Vec<char> = s.chars().collect();
    let first_char = chars[0];

    if !first_char.is_alphabetic() && first_char != '_' {
        return false;
    }

    for &ch in &chars[1..] {
        if !ch.is_alphanumeric() && ch != '_' {
            return false;
        }
    }

    true
}

fn get_indentation(line: &str) -> String {
    let mut indentation = String::new();
    for ch in line.chars() {
        if ch == ' ' || ch == '\t' {
            indentation.push(ch);
        } else {
            break;
        }
    }
    indentation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_add_end_keyword() {
        // Test if statements
        assert!(should_add_end_keyword("if condition"));
        assert!(should_add_end_keyword("  if x == y"));
        assert!(should_add_end_keyword("unless condition"));
        assert!(should_add_end_keyword("something if condition"));

        // Test while/until
        assert!(should_add_end_keyword("while condition"));
        assert!(should_add_end_keyword("until condition"));

        // Test case
        assert!(should_add_end_keyword("case variable"));

        // Test begin
        assert!(should_add_end_keyword("begin"));

        // Test class/module
        assert!(should_add_end_keyword("class MyClass"));
        assert!(should_add_end_keyword("module MyModule"));

        // Test method definitions
        assert!(should_add_end_keyword("def my_method"));
        assert!(should_add_end_keyword("def my_method(param)"));

        // Test do blocks
        assert!(should_add_end_keyword("array.each do"));
        assert!(should_add_end_keyword("array.each do |item|"));
        assert!(should_add_end_keyword("5.times do |i|"));

        // Test for loops
        assert!(should_add_end_keyword("for i in 1..10"));

        // Test cases that should NOT add end
        assert!(!should_add_end_keyword("puts 'hello'"));
        assert!(!should_add_end_keyword("x = 5"));
        assert!(!should_add_end_keyword("return if condition"));
        assert!(!should_add_end_keyword(""));
        assert!(!should_add_end_keyword("# just a comment"));

        // Test conditional assignments - these should NOT add end immediately
        assert!(!should_add_end_keyword("a = if cond"));
        assert!(!should_add_end_keyword("result = unless condition"));
        assert!(!should_add_end_keyword("value = case x"));

        // Test conditional assignments with variable spacing
        assert!(!should_add_end_keyword("a=if cond"));
        assert!(!should_add_end_keyword("result  =  unless condition"));
        assert!(!should_add_end_keyword("value   =case x"));
        assert!(!should_add_end_keyword("x = while true"));
        assert!(!should_add_end_keyword("y=until done"));

        // Test edge cases for assignments
        assert!(!should_add_end_keyword("variable_name = if something"));
        assert!(!should_add_end_keyword("_private = case value"));

        // Test that non-assignments still work
        assert!(should_add_end_keyword("if something"));
        assert!(should_add_end_keyword("case value"));
    }

    #[test]
    fn test_get_indentation() {
        assert_eq!(get_indentation("no_indent"), "");
        assert_eq!(get_indentation("  two_spaces"), "  ");
        assert_eq!(get_indentation("    four_spaces"), "    ");
        assert_eq!(get_indentation("\ttab_indent"), "\t");
        assert_eq!(get_indentation("  \tmixed_indent"), "  \t");
    }

    #[test]
    fn test_should_add_end_ast_standalone_conditionals() {
        // Standalone conditionals should trigger end addition
        assert!(should_add_end_ast("if condition"));
        assert!(should_add_end_ast("unless condition"));
        assert!(should_add_end_ast("while condition"));
        assert!(should_add_end_ast("until condition"));

        // Note: incomplete case statements like "case value" are not parsed as CaseNodes
        // so they won't be detected by AST analysis. The string-based fallback handles these.
    }

    #[test]
    fn test_should_add_end_ast_conditional_assignments() {
        // Conditional assignments should NOT trigger end addition
        assert!(!should_add_end_ast("a = if condition"));
        assert!(!should_add_end_ast("result = unless condition"));
        assert!(!should_add_end_ast("y = while condition"));
        assert!(!should_add_end_ast("z = until condition"));

        // Note: "x = case value" is not tested here because incomplete case statements
        // aren't parsed as CaseNodes, so the AST analysis doesn't detect them
    }

    #[test]
    fn test_should_add_end_ast_non_conditionals() {
        // Non-conditional code should not trigger
        assert!(!should_add_end_ast("puts 'hello'"));
        assert!(!should_add_end_ast("x = 5"));
        assert!(!should_add_end_ast("method_call"));
        assert!(!should_add_end_ast("def method_name"));
        assert!(!should_add_end_ast("class ClassName"));
    }

    #[test]
    fn test_should_add_end_ast_complex_cases() {
        // More complex scenarios
        assert!(should_add_end_ast("if x > 5"));
        assert!(!should_add_end_ast("result = if x > 5"));

        // Multiple statements - should detect the conditional
        assert!(should_add_end_ast("puts 'start'\nif condition"));
        assert!(!should_add_end_ast("puts 'start'\na = if condition"));
    }
}
