//! Return Type Inference
//!
//! Analyzes method bodies to infer return types by collecting all possible
//! return paths (explicit returns and implicit last expression).

use crate::type_inference::literal_analyzer::LiteralAnalyzer;
use crate::type_inference::ruby_type::RubyType;
use ruby_prism::*;

/// Infers return types from method bodies by analyzing all return paths
pub struct ReturnTypeInferrer {
    literal_analyzer: LiteralAnalyzer,
}

impl ReturnTypeInferrer {
    pub fn new() -> Self {
        Self {
            literal_analyzer: LiteralAnalyzer::new(),
        }
    }

    /// Infer the return type from a method body node.
    /// Returns None if no return type can be inferred.
    pub fn infer_return_type(&self, body: Option<Node>) -> Option<RubyType> {
        let body = body?;

        let mut return_types = Vec::new();

        // Collect all explicit return statements
        self.collect_return_types(&body, &mut return_types);

        // Get the implicit return type (last expression)
        if let Some(implicit_type) = self.get_implicit_return_type(&body) {
            return_types.push(implicit_type);
        }

        // If no return types found, return None
        if return_types.is_empty() {
            return None;
        }

        // Create union of all return types
        Some(RubyType::union(return_types))
    }

    /// Recursively collect return types from all explicit return statements
    fn collect_return_types(&self, node: &Node, return_types: &mut Vec<RubyType>) {
        // Check if this node is a return statement
        if let Some(return_node) = node.as_return_node() {
            if let Some(arguments) = return_node.arguments() {
                // Return with value(s)
                for arg in arguments.arguments().iter() {
                    if let Some(arg_type) = self.literal_analyzer.analyze_literal(&arg) {
                        return_types.push(arg_type);
                    } else {
                        // Try to infer from expression
                        if let Some(expr_type) = self.infer_expression_type(&arg) {
                            return_types.push(expr_type);
                        }
                    }
                }
            } else {
                // Return without value returns nil
                return_types.push(RubyType::nil_class());
            }
            return;
        }

        // Handle StatementsNode - recurse into each statement
        if let Some(statements_node) = node.as_statements_node() {
            for stmt in statements_node.body().iter() {
                self.collect_return_types(&stmt, return_types);
            }
            return;
        }

        // Handle if/unless conditionals
        if let Some(if_node) = node.as_if_node() {
            // Check then branch
            if let Some(statements) = if_node.statements() {
                self.collect_return_types(&statements.as_node(), return_types);
            }
            // Check else branch (can be another if for elsif, or statements)
            if let Some(subsequent) = if_node.subsequent() {
                self.collect_return_types(&subsequent, return_types);
            }
            return;
        }

        if let Some(unless_node) = node.as_unless_node() {
            // Check then branch
            if let Some(statements) = unless_node.statements() {
                self.collect_return_types(&statements.as_node(), return_types);
            }
            // Check else branch
            if let Some(subsequent) = unless_node.else_clause() {
                self.collect_return_types(&subsequent.as_node(), return_types);
            }
            return;
        }

        // Handle case/when
        if let Some(case_node) = node.as_case_node() {
            for condition in case_node.conditions().iter() {
                if let Some(when_node) = condition.as_when_node() {
                    if let Some(statements) = when_node.statements() {
                        self.collect_return_types(&statements.as_node(), return_types);
                    }
                }
            }
            if let Some(else_clause) = case_node.else_clause() {
                self.collect_return_types(&else_clause.as_node(), return_types);
            }
            return;
        }

        // Handle begin/rescue/ensure
        if let Some(begin_node) = node.as_begin_node() {
            if let Some(statements) = begin_node.statements() {
                self.collect_return_types(&statements.as_node(), return_types);
            }
            if let Some(rescue_clause) = begin_node.rescue_clause() {
                self.collect_return_types(&rescue_clause.as_node(), return_types);
            }
            if let Some(else_clause) = begin_node.else_clause() {
                self.collect_return_types(&else_clause.as_node(), return_types);
            }
            if let Some(ensure_clause) = begin_node.ensure_clause() {
                self.collect_return_types(&ensure_clause.as_node(), return_types);
            }
            return;
        }

        // Handle rescue modifier (expr rescue fallback)
        if let Some(rescue_modifier) = node.as_rescue_modifier_node() {
            self.collect_return_types(&rescue_modifier.expression(), return_types);
            self.collect_return_types(&rescue_modifier.rescue_expression(), return_types);
            return;
        }

        // Handle while/until loops
        if let Some(while_node) = node.as_while_node() {
            if let Some(statements) = while_node.statements() {
                self.collect_return_types(&statements.as_node(), return_types);
            }
            return;
        }

        if let Some(until_node) = node.as_until_node() {
            if let Some(statements) = until_node.statements() {
                self.collect_return_types(&statements.as_node(), return_types);
            }
            return;
        }

        // Handle for loops
        if let Some(for_node) = node.as_for_node() {
            if let Some(statements) = for_node.statements() {
                self.collect_return_types(&statements.as_node(), return_types);
            }
            return;
        }

        // Handle blocks (do...end or {...})
        if let Some(block_node) = node.as_block_node() {
            if let Some(body) = block_node.body() {
                self.collect_return_types(&body, return_types);
            }
            return;
        }

        // Handle method calls with blocks
        if let Some(call_node) = node.as_call_node() {
            if let Some(block) = call_node.block() {
                self.collect_return_types(&block, return_types);
            }
            return;
        }
    }

    /// Get the implicit return type from the last expression in the body
    fn get_implicit_return_type(&self, node: &Node) -> Option<RubyType> {
        // Handle StatementsNode - get the last statement
        if let Some(statements_node) = node.as_statements_node() {
            let statements: Vec<_> = statements_node.body().iter().collect();
            if let Some(last_stmt) = statements.last() {
                return self.get_expression_return_type(last_stmt);
            }
            return None;
        }

        // For other nodes, try to get their return type directly
        self.get_expression_return_type(node)
    }

    /// Get the return type of an expression (for implicit returns)
    fn get_expression_return_type(&self, node: &Node) -> Option<RubyType> {
        // Handle control flow structures FIRST - before trying literal analysis
        // because they are not literals but have return types

        // Handle if/unless - return union of all branches
        if let Some(ref if_node) = node.as_if_node() {
            return self.get_conditional_return_type_if(if_node);
        }

        if let Some(ref unless_node) = node.as_unless_node() {
            return self.get_conditional_return_type_unless(unless_node);
        }

        // Handle else clause (from if/unless subsequent)
        if let Some(ref else_node) = node.as_else_node() {
            if let Some(statements) = else_node.statements() {
                return self.get_implicit_return_type(&statements.as_node());
            }
            return Some(RubyType::nil_class());
        }

        // Handle case/when
        if let Some(ref case_node) = node.as_case_node() {
            return self.get_case_return_type(case_node);
        }

        // Handle begin/rescue
        if let Some(ref begin_node) = node.as_begin_node() {
            return self.get_begin_return_type(begin_node);
        }

        // Try literal analysis for simple values
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(node) {
            return Some(literal_type);
        }

        // Handle and/or expressions
        if let Some(and_node) = node.as_and_node() {
            let left_type = self.get_expression_return_type(&and_node.left());
            let right_type = self.get_expression_return_type(&and_node.right());
            match (left_type, right_type) {
                (Some(l), Some(r)) => Some(RubyType::union([l, r])),
                (Some(l), None) => Some(l),
                (None, Some(r)) => Some(r),
                (None, None) => None,
            }
        } else if let Some(or_node) = node.as_or_node() {
            let left_type = self.get_expression_return_type(&or_node.left());
            let right_type = self.get_expression_return_type(&or_node.right());
            match (left_type, right_type) {
                (Some(l), Some(r)) => Some(RubyType::union([l, r])),
                (Some(l), None) => Some(l),
                (None, Some(r)) => Some(r),
                (None, None) => None,
            }
        } else {
            // Try to infer from expression
            self.infer_expression_type(node)
        }
    }

    /// Get return type from if/elsif/else chain
    fn get_conditional_return_type_if(&self, if_node: &IfNode) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Then branch
        if let Some(statements) = if_node.statements() {
            if let Some(then_type) = self.get_implicit_return_type(&statements.as_node()) {
                branch_types.push(then_type);
            }
        } else {
            // Empty then branch returns nil
            branch_types.push(RubyType::nil_class());
        }

        // Else branch (can be elsif or else)
        if let Some(subsequent) = if_node.subsequent() {
            if let Some(else_type) = self.get_expression_return_type(&subsequent) {
                branch_types.push(else_type);
            }
        } else {
            // No else branch means the if can return nil
            branch_types.push(RubyType::nil_class());
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Get return type from unless/else
    fn get_conditional_return_type_unless(&self, unless_node: &UnlessNode) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Then branch (the unless body)
        if let Some(statements) = unless_node.statements() {
            if let Some(then_type) = self.get_implicit_return_type(&statements.as_node()) {
                branch_types.push(then_type);
            }
        } else {
            branch_types.push(RubyType::nil_class());
        }

        // Else branch
        if let Some(else_clause) = unless_node.else_clause() {
            if let Some(else_type) = self.get_expression_return_type(&else_clause.as_node()) {
                branch_types.push(else_type);
            }
        } else {
            branch_types.push(RubyType::nil_class());
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Get return type from case/when
    fn get_case_return_type(&self, case_node: &CaseNode) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                if let Some(statements) = when_node.statements() {
                    if let Some(when_type) = self.get_implicit_return_type(&statements.as_node()) {
                        branch_types.push(when_type);
                    }
                } else {
                    branch_types.push(RubyType::nil_class());
                }
            }
        }

        // Else clause
        if let Some(else_clause) = case_node.else_clause() {
            if let Some(else_type) = self.get_expression_return_type(&else_clause.as_node()) {
                branch_types.push(else_type);
            }
        } else {
            // No else means case can return nil
            branch_types.push(RubyType::nil_class());
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Get return type from begin/rescue/else/ensure
    fn get_begin_return_type(&self, begin_node: &BeginNode) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Main statements (or else clause if present)
        if let Some(else_clause) = begin_node.else_clause() {
            if let Some(else_type) = self.get_expression_return_type(&else_clause.as_node()) {
                branch_types.push(else_type);
            }
        } else if let Some(statements) = begin_node.statements() {
            if let Some(main_type) = self.get_implicit_return_type(&statements.as_node()) {
                branch_types.push(main_type);
            }
        }

        // Rescue clause can also be the return value
        if let Some(rescue_clause) = begin_node.rescue_clause() {
            if let Some(rescue_type) = self.get_rescue_return_type(&rescue_clause) {
                branch_types.push(rescue_type);
            }
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Get return type from rescue clause chain
    fn get_rescue_return_type(&self, rescue_node: &RescueNode) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // This rescue clause's statements
        if let Some(statements) = rescue_node.statements() {
            if let Some(rescue_type) = self.get_implicit_return_type(&statements.as_node()) {
                branch_types.push(rescue_type);
            }
        } else {
            branch_types.push(RubyType::nil_class());
        }

        // Subsequent rescue clauses
        if let Some(subsequent) = rescue_node.subsequent() {
            if let Some(subsequent_type) = self.get_rescue_return_type(&subsequent) {
                branch_types.push(subsequent_type);
            }
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Try to infer type from non-literal expressions
    fn infer_expression_type(&self, node: &Node) -> Option<RubyType> {
        // Parenthesized expression
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.get_expression_return_type(&body);
            }
            return Some(RubyType::nil_class());
        }

        // Interpolated string always returns String
        if node.as_interpolated_string_node().is_some() {
            return Some(RubyType::string());
        }

        // For now, we can't infer method calls or variable references
        // This will be handled in Milestone 5
        None
    }
}

impl Default for ReturnTypeInferrer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infer_return_type(source: &str) -> Option<RubyType> {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();
        let inferrer = ReturnTypeInferrer::new();

        // Find the def node and get its body
        if let Some(program) = ast.as_program_node() {
            let statements = program.statements();
            for stmt in statements.body().iter() {
                if let Some(def_node) = stmt.as_def_node() {
                    return inferrer.infer_return_type(def_node.body());
                }
            }
        }
        None
    }

    // =========================================================================
    // Simple literal returns
    // =========================================================================

    #[test]
    fn test_simple_string_return() {
        let source = r#"
def greet
  "hello"
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::string()));
    }

    #[test]
    fn test_simple_integer_return() {
        let source = r#"
def answer
  42
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::integer()));
    }

    #[test]
    fn test_simple_float_return() {
        let source = r#"
def pi
  3.14
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::float()));
    }

    #[test]
    fn test_simple_symbol_return() {
        let source = r#"
def status
  :ok
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::symbol()));
    }

    #[test]
    fn test_simple_true_return() {
        let source = r#"
def valid?
  true
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::true_class()));
    }

    #[test]
    fn test_simple_false_return() {
        let source = r#"
def invalid?
  false
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::false_class()));
    }

    #[test]
    fn test_simple_nil_return() {
        let source = r#"
def nothing
  nil
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::nil_class()));
    }

    #[test]
    fn test_simple_array_return() {
        let source = r#"
def numbers
  [1, 2, 3]
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::Array(vec![RubyType::integer()])));
    }

    #[test]
    fn test_simple_hash_return() {
        let source = r#"
def config
  {name: "app", debug: true}
end
"#;
        let result = infer_return_type(source);
        // Hash with symbol keys and string/boolean values
        assert!(matches!(result, Some(RubyType::Hash(_, _))));
    }

    // =========================================================================
    // Explicit return statements
    // =========================================================================

    #[test]
    fn test_explicit_return() {
        let source = r#"
def greet
  return "hello"
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::string()));
    }

    #[test]
    fn test_explicit_return_no_value() {
        let source = r#"
def nothing
  return
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::nil_class()));
    }

    // =========================================================================
    // Early returns with different types
    // =========================================================================

    #[test]
    fn test_early_return_nil() {
        let source = r#"
def process(value)
  return nil if value.nil?
  "processed"
end
"#;
        let result = infer_return_type(source);
        // Should be union of nil and string
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::nil_class()));
            assert!(types.contains(&RubyType::string()));
        }
    }

    #[test]
    fn test_early_return_different_type() {
        let source = r#"
def process(value)
  return "error" if value < 0
  42
end
"#;
        let result = infer_return_type(source);
        // Should be union of string and integer
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::integer()));
        }
    }

    #[test]
    fn test_multiple_early_returns() {
        let source = r#"
def process(value)
  return nil if value.nil?
  return "error" if value < 0
  42
end
"#;
        let result = infer_return_type(source);
        // Should be union of nil, string, and integer
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::nil_class()));
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::integer()));
        }
    }

    // =========================================================================
    // Conditional returns (if/else)
    // =========================================================================

    #[test]
    fn test_if_else_same_type() {
        let source = r#"
def greet(formal)
  if formal
    "Good day"
  else
    "Hey"
  end
end
"#;
        let result = infer_return_type(source);
        // Both branches return String, so result should be String
        assert_eq!(result, Some(RubyType::string()));
    }

    #[test]
    fn test_if_else_different_types() {
        let source = r#"
def process(flag)
  if flag
    "yes"
  else
    42
  end
end
"#;
        let result = infer_return_type(source);
        // Should be union of string and integer
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::integer()));
        }
    }

    #[test]
    fn test_if_without_else() {
        let source = r#"
def maybe_greet(flag)
  if flag
    "hello"
  end
end
"#;
        let result = infer_return_type(source);
        // Should be union of string and nil (no else means nil)
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::nil_class()));
        }
    }

    #[test]
    fn test_if_elsif_else() {
        let source = r#"
def status(code)
  if code == 200
    :ok
  elsif code == 404
    :not_found
  else
    :error
  end
end
"#;
        let result = infer_return_type(source);
        // All branches return Symbol
        assert_eq!(result, Some(RubyType::symbol()));
    }

    #[test]
    fn test_unless_else() {
        let source = r#"
def process(value)
  unless value
    nil
  else
    "processed"
  end
end
"#;
        let result = infer_return_type(source);
        // Should be union of nil and string
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::nil_class()));
            assert!(types.contains(&RubyType::string()));
        }
    }

    // =========================================================================
    // Case/when returns
    // =========================================================================

    #[test]
    fn test_case_when_same_type() {
        let source = r#"
def day_name(num)
  case num
  when 1 then "Monday"
  when 2 then "Tuesday"
  else "Unknown"
  end
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::string()));
    }

    #[test]
    fn test_case_when_different_types() {
        let source = r#"
def process(type)
  case type
  when :string then "hello"
  when :number then 42
  else nil
  end
end
"#;
        let result = infer_return_type(source);
        // Should be union of string, integer, and nil
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::integer()));
            assert!(types.contains(&RubyType::nil_class()));
        }
    }

    #[test]
    fn test_case_when_no_else() {
        let source = r#"
def status(code)
  case code
  when 200 then :ok
  when 404 then :not_found
  end
end
"#;
        let result = infer_return_type(source);
        // Should be union of symbol and nil (no else means nil possible)
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::symbol()));
            assert!(types.contains(&RubyType::nil_class()));
        }
    }

    // =========================================================================
    // Begin/rescue returns
    // =========================================================================

    #[test]
    fn test_begin_rescue_same_type() {
        let source = r#"
def safe_parse(str)
  begin
    "parsed"
  rescue
    "error"
  end
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::string()));
    }

    #[test]
    fn test_begin_rescue_different_types() {
        let source = r#"
def safe_parse(str)
  begin
    42
  rescue
    nil
  end
end
"#;
        let result = infer_return_type(source);
        // Should be union of integer and nil
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::integer()));
            assert!(types.contains(&RubyType::nil_class()));
        }
    }

    // =========================================================================
    // Empty method body
    // =========================================================================

    #[test]
    fn test_empty_method() {
        let source = r#"
def nothing
end
"#;
        let result = infer_return_type(source);
        // Empty method returns nil
        assert_eq!(result, None);
    }

    // =========================================================================
    // Multiple statements with last expression
    // =========================================================================

    #[test]
    fn test_multiple_statements() {
        let source = r#"
def process
  x = 1
  y = 2
  "result"
end
"#;
        let result = infer_return_type(source);
        // Last expression is the return value
        assert_eq!(result, Some(RubyType::string()));
    }

    // =========================================================================
    // Interpolated strings
    // =========================================================================

    #[test]
    fn test_interpolated_string() {
        let source = r#"
def greet(name)
  "Hello, #{name}!"
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::string()));
    }

    // =========================================================================
    // Guard clauses
    // =========================================================================

    #[test]
    fn test_guard_clause_return() {
        let source = r#"
def fetch(id)
  return unless id
  "found"
end
"#;
        let result = infer_return_type(source);
        // return without value returns nil
        assert!(matches!(result, Some(RubyType::Union(_))));
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::nil_class()));
            assert!(types.contains(&RubyType::string()));
        }
    }
}
