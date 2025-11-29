//! Return Type Inference using CFG-based dataflow analysis.
//!
//! This module infers return types from method bodies by:
//! 1. Building a Control Flow Graph (CFG) from the method AST
//! 2. Running dataflow analysis to propagate type narrowing
//! 3. Collecting return types from all exit paths with proper narrowed types

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::type_inference::cfg::{CfgBuilder, DataflowAnalyzer, StatementKind, TypeState};
use crate::type_inference::literal_analyzer::LiteralAnalyzer;
use crate::type_inference::rbs_index::get_rbs_method_return_type_as_ruby_type;
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use parking_lot::Mutex;
use ruby_prism::*;
use std::sync::Arc;

/// Infers return types from method bodies using CFG-based dataflow analysis.
/// This properly handles type narrowing in control flow structures like case/when.
pub struct ReturnTypeInferrer {
    index: Arc<Mutex<RubyIndex>>,
    literal_analyzer: LiteralAnalyzer,
}

impl ReturnTypeInferrer {
    /// Create a new return type inferrer with access to the Ruby index
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
        }
    }

    /// Infer the return type from a method definition using CFG analysis.
    /// This properly handles type narrowing in control flow structures.
    pub fn infer_return_type(&self, source: &[u8], method: &DefNode) -> Option<RubyType> {
        // Build CFG from the method
        let builder = CfgBuilder::new(source);
        let cfg = builder.build_from_method(method);

        // Run dataflow analysis with initial parameter types
        let mut analyzer = DataflowAnalyzer::new(&cfg);
        let initial_state = TypeState::from_parameters(&cfg.parameters);
        analyzer.analyze(initial_state);
        let results = analyzer.into_results();

        // Collect all possible return types from exit blocks
        let mut return_types = Vec::new();

        for exit_id in &cfg.exits {
            if let Some(block) = cfg.blocks.get(exit_id) {
                // Get the type state at this exit point for narrowed variable types
                let exit_state = results.get_exit_state(*exit_id);

                for stmt in &block.statements {
                    match &stmt.kind {
                        StatementKind::Return { value_type } => {
                            if let Some(ty) = value_type {
                                return_types.push(ty.clone());
                            } else {
                                return_types.push(RubyType::nil_class());
                            }
                        }
                        StatementKind::Expression => {
                            // This might be an implicit return - we need to analyze the
                            // actual expression. For now, we'll try to infer from the
                            // method body's last expression.
                        }
                        StatementKind::MethodCall { receiver, method } => {
                            // Method call might be an implicit return
                            if let Some(recv_type) = self.get_receiver_type(receiver, exit_state) {
                                if let Some(return_type) =
                                    self.lookup_method_return_type(&recv_type, method)
                                {
                                    return_types.push(return_type);
                                }
                            }
                        }
                        StatementKind::Assignment { .. }
                        | StatementKind::OrAssignment { .. }
                        | StatementKind::AndAssignment { .. } => {
                            // Assignments are not return values
                        }
                    }
                }
            }
        }

        // Always analyze the method body for implicit return
        // (the last expression in the method is an implicit return if control flow reaches it)
        if let Some(body) = method.body() {
            if let Some(implicit_type) = self.infer_implicit_return(&body, &cfg, &results) {
                return_types.push(implicit_type);
            }
        }

        if return_types.is_empty() {
            // Method with no body returns nil
            Some(RubyType::nil_class())
        } else {
            Some(RubyType::union(return_types))
        }
    }

    /// Infer the implicit return type from the last expression in the method body
    fn infer_implicit_return(
        &self,
        body: &Node,
        cfg: &crate::type_inference::cfg::ControlFlowGraph,
        results: &crate::type_inference::cfg::DataflowResults,
    ) -> Option<RubyType> {
        // Get the last statement's type
        if let Some(statements) = body.as_statements_node() {
            let stmts: Vec<_> = statements.body().iter().collect();
            if let Some(last_stmt) = stmts.last() {
                // For non-control-flow statements, find the block and use its state
                let stmt_offset = last_stmt.location().start_offset();
                for (block_id, block) in &cfg.blocks {
                    if block.location.start_offset <= stmt_offset
                        && stmt_offset <= block.location.end_offset
                    {
                        let exit_state = results.get_exit_state(*block_id);
                        return self.infer_expression_type(last_stmt, exit_state);
                    }
                }

                // Fallback: try to infer without narrowed state
                return self.infer_expression_type(last_stmt, None);
            }
        }

        // Try direct expression type inference
        self.infer_expression_type(body, None)
    }

    /// Infer the type of an expression using narrowed type state from dataflow analysis
    fn infer_expression_type(&self, node: &Node, state: Option<&TypeState>) -> Option<RubyType> {
        // First try literal analysis
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(node) {
            return Some(literal_type);
        }

        // Handle method calls with narrowed receiver types
        if let Some(call_node) = node.as_call_node() {
            return self.infer_call_type(&call_node, state);
        }

        // Handle local variable reads with narrowed types
        if let Some(local_var) = node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(local_var.name().as_slice()).to_string();
            if let Some(type_state) = state {
                if let Some(ty) = type_state.get_type(&var_name) {
                    return Some(ty.clone());
                }
            }
        }

        // Handle control flow structures
        if let Some(if_node) = node.as_if_node() {
            return self.infer_if_return_type(&if_node, state);
        }

        if let Some(case_node) = node.as_case_node() {
            return self.infer_case_return_type(&case_node, state);
        }

        // Handle parenthesized expressions
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.infer_expression_type(&body, state);
            }
            return Some(RubyType::nil_class());
        }

        // Interpolated string always returns String
        if node.as_interpolated_string_node().is_some() {
            return Some(RubyType::string());
        }

        // Handle statements node (get last statement's type)
        if let Some(statements) = node.as_statements_node() {
            let stmts: Vec<_> = statements.body().iter().collect();
            if let Some(last_stmt) = stmts.last() {
                return self.infer_expression_type(last_stmt, state);
            }
        }

        // Handle else node (from if/unless)
        if let Some(else_node) = node.as_else_node() {
            if let Some(statements) = else_node.statements() {
                return self.infer_expression_type(&statements.as_node(), state);
            }
            return Some(RubyType::nil_class());
        }

        None
    }

    /// Infer return type from an if/elsif/else chain
    fn infer_if_return_type(
        &self,
        if_node: &IfNode,
        state: Option<&TypeState>,
    ) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Then branch
        if let Some(statements) = if_node.statements() {
            if let Some(then_type) = self.infer_expression_type(&statements.as_node(), state) {
                branch_types.push(then_type);
            }
        } else {
            branch_types.push(RubyType::nil_class());
        }

        // Else branch
        if let Some(subsequent) = if_node.subsequent() {
            if let Some(else_type) = self.infer_expression_type(&subsequent, state) {
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

    /// Infer return type from a case/when statement with proper type narrowing
    fn infer_case_return_type(
        &self,
        case_node: &CaseNode,
        _parent_state: Option<&TypeState>,
    ) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Get the case predicate variable name
        let case_var = case_node
            .predicate()
            .and_then(|p| self.extract_variable_name(&p));

        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                // Extract the pattern type from the when clause
                let pattern_type = self.extract_when_pattern_type(&when_node);

                // Create a narrowed state for this branch
                let narrowed_state = if let (Some(ref var_name), Some(ref narrowed_type)) =
                    (&case_var, &pattern_type)
                {
                    let mut state = TypeState::new();
                    state.set_type(var_name.clone(), narrowed_type.clone());
                    Some(state)
                } else {
                    None
                };

                if let Some(statements) = when_node.statements() {
                    if let Some(when_type) =
                        self.infer_expression_type(&statements.as_node(), narrowed_state.as_ref())
                    {
                        branch_types.push(when_type);
                    }
                } else {
                    branch_types.push(RubyType::nil_class());
                }
            }
        }

        // Else clause
        if let Some(else_clause) = case_node.else_clause() {
            if let Some(else_type) = self.infer_expression_type(&else_clause.as_node(), None) {
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

    /// Infer the return type of a method call using narrowed receiver type
    fn infer_call_type(&self, call_node: &CallNode, state: Option<&TypeState>) -> Option<RubyType> {
        let method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();

        // Get receiver type (possibly narrowed)
        let receiver_type = if let Some(receiver) = call_node.receiver() {
            self.get_node_receiver_type(&receiver, state)
        } else {
            None
        };

        if let Some(recv_type) = receiver_type {
            return self.lookup_method_return_type(&recv_type, &method_name);
        }

        None
    }

    /// Get the type of a receiver expression, using narrowed types from state
    fn get_node_receiver_type(&self, node: &Node, state: Option<&TypeState>) -> Option<RubyType> {
        // First try literal analysis
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(node) {
            return Some(literal_type);
        }

        // Handle local variable with narrowed type
        if let Some(local_var) = node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(local_var.name().as_slice()).to_string();
            if let Some(type_state) = state {
                if let Some(ty) = type_state.get_type(&var_name) {
                    return Some(ty.clone());
                }
            }
            // Try to look up in index
            return self.lookup_local_variable_type(&var_name);
        }

        // Handle chained method calls
        if let Some(call) = node.as_call_node() {
            return self.infer_call_type(&call, state);
        }

        None
    }

    /// Get receiver type from a string name (for CFG statements)
    fn get_receiver_type(
        &self,
        receiver: &Option<String>,
        state: Option<&TypeState>,
    ) -> Option<RubyType> {
        let var_name = receiver.as_ref()?;

        // Check narrowed state first
        if let Some(type_state) = state {
            if let Some(ty) = type_state.get_type(var_name) {
                return Some(ty.clone());
            }
        }

        // Fall back to index lookup
        self.lookup_local_variable_type(var_name)
    }

    /// Look up method return type using RBS
    fn lookup_method_return_type(
        &self,
        recv_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let class_name = self.get_class_name_for_rbs(recv_type)?;
        get_rbs_method_return_type_as_ruby_type(&class_name, method_name, false)
    }

    /// Look up a local variable's type from the index.
    /// This is a fallback when the CFG doesn't have type information.
    fn lookup_local_variable_type(&self, var_name: &str) -> Option<RubyType> {
        let index = self.index.lock();

        for (fqn, entries) in &index.definitions {
            if let FullyQualifiedName::LocalVariable(name, _) = fqn {
                if name == var_name {
                    for entry in entries {
                        if let EntryKind::LocalVariable { r#type, .. } = &entry.kind {
                            if *r#type != RubyType::Unknown {
                                return Some(r#type.clone());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Get class name for RBS lookup from a RubyType
    fn get_class_name_for_rbs(&self, ruby_type: &RubyType) -> Option<String> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
                if let FullyQualifiedName::Constant(parts) = fqn {
                    parts.last().map(|c| c.to_string())
                } else {
                    None
                }
            }
            RubyType::Array(_) => Some("Array".to_string()),
            RubyType::Hash(_, _) => Some("Hash".to_string()),
            _ => None,
        }
    }

    /// Extract variable name from a node
    fn extract_variable_name(&self, node: &Node) -> Option<String> {
        if let Some(local_var) = node.as_local_variable_read_node() {
            return Some(String::from_utf8_lossy(local_var.name().as_slice()).to_string());
        }
        None
    }

    /// Extract pattern type from a when clause
    fn extract_when_pattern_type(&self, when_node: &WhenNode) -> Option<RubyType> {
        let first_condition = when_node.conditions().iter().next()?;

        // Handle constant like String, Integer
        if let Some(const_read) = first_condition.as_constant_read_node() {
            let name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
            return Some(self.constant_to_ruby_type(&name));
        }

        // Handle nil literal
        if first_condition.as_nil_node().is_some() {
            return Some(RubyType::nil_class());
        }

        // Handle true literal
        if first_condition.as_true_node().is_some() {
            return Some(RubyType::true_class());
        }

        // Handle false literal
        if first_condition.as_false_node().is_some() {
            return Some(RubyType::false_class());
        }

        None
    }

    /// Convert constant name to RubyType
    fn constant_to_ruby_type(&self, name: &str) -> RubyType {
        match name {
            "String" => RubyType::string(),
            "Integer" => RubyType::integer(),
            "Float" => RubyType::float(),
            "Symbol" => RubyType::symbol(),
            "TrueClass" => RubyType::true_class(),
            "FalseClass" => RubyType::false_class(),
            "NilClass" => RubyType::nil_class(),
            "Array" => RubyType::Array(vec![RubyType::Any]),
            "Hash" => RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]),
            _ => RubyType::class(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_inferrer() -> ReturnTypeInferrer {
        let index = Arc::new(Mutex::new(crate::indexer::index::RubyIndex::new()));
        ReturnTypeInferrer::new(index)
    }

    fn infer_return_type(source: &str) -> Option<RubyType> {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();
        let inferrer = create_test_inferrer();

        if let Some(program) = ast.as_program_node() {
            let statements = program.statements();
            for stmt in statements.body().iter() {
                if let Some(def_node) = stmt.as_def_node() {
                    return inferrer.infer_return_type(source.as_bytes(), &def_node);
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
def active?
  true
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::true_class()));
    }

    #[test]
    fn test_simple_false_return() {
        let source = r#"
def disabled?
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

    // =========================================================================
    // Case/when with type narrowing
    // =========================================================================

    #[test]
    fn test_case_when_with_method_calls() {
        let source = r#"
def process(value)
  case value
  when String
    value.upcase
  when Integer
    value + 1
  when nil
    "nil"
  end
end
"#;
        let result = infer_return_type(source);

        assert!(result.is_some(), "Expected a return type");
        let result = result.unwrap();

        if let RubyType::Union(types) = &result {
            assert!(
                types.contains(&RubyType::string()),
                "Expected String in union, got: {:?}",
                types
            );
            assert!(
                types.contains(&RubyType::integer()),
                "Expected Integer in union, got: {:?}",
                types
            );
            assert!(
                types.contains(&RubyType::nil_class()),
                "Expected NilClass in union, got: {:?}",
                types
            );
        } else {
            panic!("Expected Union type, got: {:?}", result);
        }
    }

    #[test]
    fn test_string_method_return_type() {
        let source = r#"
def get_upper
  "hello".upcase
end
"#;
        let result = infer_return_type(source);
        assert_eq!(
            result,
            Some(RubyType::string()),
            "String#upcase should return String"
        );
    }

    #[test]
    fn test_integer_addition_return_type() {
        let source = r#"
def add_one
  1 + 1
end
"#;
        let result = infer_return_type(source);
        assert_eq!(
            result,
            Some(RubyType::integer()),
            "Integer#+ should return Integer"
        );
    }

    // =========================================================================
    // If/else branches
    // =========================================================================

    #[test]
    fn test_if_else_different_types() {
        let source = r#"
def maybe_number(flag)
  if flag
    42
  else
    "not a number"
  end
end
"#;
        let result = infer_return_type(source);
        assert!(result.is_some());
        if let Some(RubyType::Union(types)) = &result {
            assert!(types.contains(&RubyType::integer()));
            assert!(types.contains(&RubyType::string()));
        } else {
            panic!("Expected Union type, got: {:?}", result);
        }
    }

    #[test]
    fn test_if_without_else() {
        let source = r#"
def maybe_string(flag)
  if flag
    "hello"
  end
end
"#;
        let result = infer_return_type(source);
        assert!(result.is_some());
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::nil_class()));
        } else {
            panic!("Expected Union type");
        }
    }

    // =========================================================================
    // Empty method
    // =========================================================================

    #[test]
    fn test_empty_method() {
        let source = r#"
def empty
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::nil_class()));
    }

    // =========================================================================
    // Mixed explicit return and implicit return
    // =========================================================================

    #[test]
    fn test_explicit_and_implicit_return() {
        // This method has:
        // 1. An explicit `return 1.0` in the if branch
        // 2. An implicit return from the case statement
        let source = r#"
def process(value)
    if rand > 0.5
        return 1.0
    end

    case value
    when String
        value.upcase
    when Integer
        value + 1
    when nil
        "nil"
    end
end
"#;
        let result = infer_return_type(source);
        assert!(result.is_some(), "Expected a return type");
        let result = result.unwrap();

        if let RubyType::Union(types) = &result {
            // Should contain Float (from explicit return 1.0)
            assert!(
                types.contains(&RubyType::float()),
                "Expected Float in union, got: {:?}",
                types
            );
            // Should contain String (from upcase and "nil")
            assert!(
                types.contains(&RubyType::string()),
                "Expected String in union, got: {:?}",
                types
            );
            // Should contain Integer (from value + 1)
            assert!(
                types.contains(&RubyType::integer()),
                "Expected Integer in union, got: {:?}",
                types
            );
            // Should contain NilClass (from no else in case)
            assert!(
                types.contains(&RubyType::nil_class()),
                "Expected NilClass in union, got: {:?}",
                types
            );
        } else {
            panic!("Expected Union type, got: {:?}", result);
        }
    }
}
