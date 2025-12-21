//! Return Type Inference using CFG-based dataflow analysis.
//!
//! This module infers return types from method bodies by:
//! 1. Building a Control Flow Graph (CFG) from the method AST
//! 2. Running dataflow analysis to propagate type narrowing
//! 3. Collecting return types from all exit paths with proper narrowed types

use crate::indexer::entry::{entry_kind::EntryKind, MethodKind};
use crate::indexer::index::RubyIndex;
use crate::type_inference::cfg::{CfgBuilder, DataflowAnalyzer, StatementKind, TypeState};
use crate::type_inference::literal_analyzer::LiteralAnalyzer;
use crate::type_inference::rbs_index::get_rbs_method_return_type_as_ruby_type;
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use parking_lot::Mutex;
use ruby_prism::*;
use std::sync::Arc;

/// Infers return types from method bodies using CFG-based dataflow analysis.
/// This properly handles type narrowing in control flow structures like case/when.
pub struct ReturnTypeInferrer {
    index: Arc<Mutex<RubyIndex>>,
    literal_analyzer: LiteralAnalyzer,
    /// Optional file content for on-demand inference of called methods
    content: Option<Vec<u8>>,
    /// Track methods currently being inferred to prevent infinite recursion
    inference_in_progress: std::cell::RefCell<std::collections::HashSet<String>>,
}

impl ReturnTypeInferrer {
    /// Create a new return type inferrer with access to the Ruby index
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
            content: None,
            inference_in_progress: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Create a new return type inferrer with file content for on-demand inference
    /// When looking up a method that has no return type, it will infer it automatically
    pub fn new_with_content(index: Arc<Mutex<RubyIndex>>, content: &[u8]) -> Self {
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
            content: Some(content.to_vec()),
            inference_in_progress: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Infer return types and their locations from valid return points.
    pub fn infer_return_values(
        &self,
        source: &[u8],
        method: &DefNode,
    ) -> Vec<(RubyType, usize, usize)> {
        // Build CFG from the method
        let builder = CfgBuilder::new(source);
        let cfg = builder.build_from_method(method);

        // Run dataflow analysis with initial parameter types
        let mut analyzer = DataflowAnalyzer::new(&cfg);
        let initial_state = TypeState::from_parameters(&cfg.parameters);
        analyzer.analyze(initial_state);
        let results = analyzer.into_results();

        // Collect all possible return types from exit blocks
        let mut return_values = Vec::new();

        // Collect ReturnNodes from the method body for re-inference
        let return_nodes = self.collect_return_nodes(method);

        for exit_id in &cfg.exits {
            if let Some(block) = cfg.blocks.get(exit_id) {
                for stmt in &block.statements {
                    match &stmt.kind {
                        StatementKind::Return { value_type } => {
                            let mut ty = value_type.clone();

                            // If CFG builder didn't infer type (e.g. method calls), try to re-infer from AST
                            if ty.is_none() {
                                // Find the corresponding ReturnNode by matching offset
                                for ret_node in &return_nodes {
                                    let loc = ret_node.location();
                                    if loc.start_offset() == stmt.start_offset
                                        && loc.end_offset() == stmt.end_offset
                                    {
                                        if let Some(args) = ret_node.arguments() {
                                            let args_list: Vec<_> =
                                                args.arguments().iter().collect();
                                            if let Some(first_arg) = args_list.first() {
                                                let exit_state = results.get_exit_state(*exit_id);
                                                ty = self
                                                    .infer_expression_type(first_arg, exit_state);
                                            }
                                        }
                                        break;
                                    }
                                }
                            }

                            let final_ty = ty.unwrap_or(RubyType::nil_class());
                            return_values.push((final_ty, stmt.start_offset, stmt.end_offset));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Always analyze the method body for implicit return
        // (the last expression in the method is an implicit return if control flow reaches it)
        if let Some(body) = method.body() {
            if let Some((implicit_type, start, end)) =
                self.infer_implicit_return_with_loc(&body, &cfg, &results)
            {
                return_values.push((implicit_type, start, end));
            }
        }

        // If no returns found and method has empty body or just doesn't return anything explicit/implicit detected
        if return_values.is_empty() {
            // If body is empty, it returns nil.
            // We use method name location or end location as a fallback?
            // Or maybe we don't return anything if we can't find a location.
            // Actually empty method returns nil.
            let loc = method.name_loc();
            return_values.push((RubyType::nil_class(), loc.start_offset(), loc.end_offset()));
        }

        return_values
    }

    /// Infer the return type from a method definition using CFG analysis.
    /// This properly handles type narrowing in control flow structures.
    pub fn infer_return_type(&self, source: &[u8], method: &DefNode) -> Option<RubyType> {
        let values = self.infer_return_values(source, method);
        if values.is_empty() {
            None
        } else {
            let types: Vec<RubyType> = values.into_iter().map(|(t, _, _)| t).collect();
            Some(RubyType::union(types))
        }
    }

    fn infer_implicit_return_with_loc(
        &self,
        body: &Node,
        cfg: &crate::type_inference::cfg::ControlFlowGraph,
        results: &crate::type_inference::cfg::DataflowResults,
    ) -> Option<(RubyType, usize, usize)> {
        // Get the last statement's type
        if let Some(statements) = body.as_statements_node() {
            let stmts: Vec<_> = statements.body().iter().collect();
            if let Some(last_stmt) = stmts.last() {
                let start = last_stmt.location().start_offset();
                let end = last_stmt.location().end_offset();

                // For non-control-flow statements, find the block and use its state
                let stmt_offset = start;
                for (block_id, block) in &cfg.blocks {
                    if block.location.start_offset <= stmt_offset
                        && stmt_offset <= block.location.end_offset
                    {
                        let exit_state = results.get_exit_state(*block_id);
                        if let Some(ty) = self.infer_expression_type(last_stmt, exit_state) {
                            return Some((ty, start, end));
                        }
                    }
                }

                // Fallback: try to infer without narrowed state
                if let Some(ty) = self.infer_expression_type(last_stmt, None) {
                    return Some((ty, start, end));
                }
            }
        }

        // Try direct expression type inference
        let start = body.location().start_offset();
        let end = body.location().end_offset();
        self.infer_expression_type(body, None)
            .map(|ty| (ty, start, end))
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
        } else {
            // Implicit self call - look up in index for any method with this name
            // TODO: scoping is tricky here without context. We just look for any method with this name.
            return self.lookup_method_in_index(&method_name, None);
        }
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

    /// Look up method return type using RBS
    fn lookup_method_return_type(
        &self,
        recv_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let class_name = self.get_class_name_for_rbs(recv_type);
        // Try RBS first if class name is known
        if let Some(ref name) = class_name {
            if let Some(ty) = get_rbs_method_return_type_as_ruby_type(name, method_name, false) {
                return Some(ty);
            }
        }

        // Try Index
        self.lookup_method_in_index(method_name, class_name.as_deref())
    }

    /// Look up method in the index by name and return its return type.
    /// If the method has no return type and we have file content, infer it on-demand.
    fn lookup_method_in_index(
        &self,
        method_name: &str,
        _owner_class: Option<&str>,
    ) -> Option<RubyType> {
        // Check if we're already inferring this method (cycle detection)
        {
            let in_progress = self.inference_in_progress.borrow();
            if in_progress.contains(method_name) {
                // Cycle detected - return None to break the recursion
                return None;
            }
        }

        let index = self.index.lock();
        let mut return_types = Vec::new();
        let mut methods_needing_inference: Vec<(u32, crate::indexer::index::EntryId)> = Vec::new();

        let kinds = [MethodKind::Instance, MethodKind::Class];

        for kind in kinds {
            if let Ok(method_key) = RubyMethod::new(method_name, kind) {
                if let Some(entry_ids) = index.get_method_ids(&method_key) {
                    for entry_id in entry_ids {
                        if let Some(entry) = index.get_entry(*entry_id) {
                            if let EntryKind::Method(data) = &entry.kind {
                                if let Some(rt) = &data.return_type {
                                    if *rt != RubyType::Unknown {
                                        return_types.push(rt.clone());
                                    }
                                } else if self.content.is_some() {
                                    // Method has no return type - collect for on-demand inference
                                    if let Some(pos) = data.return_type_position {
                                        methods_needing_inference.push((pos.line, *entry_id));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Quick return if we found types
        if !return_types.is_empty() {
            return Some(RubyType::union(return_types));
        }

        // Drop the index lock before inference
        drop(index);

        // If we have content and methods need inference, infer them on-demand
        if let Some(content) = &self.content {
            // Mark this method as being inferred (cycle detection)
            self.inference_in_progress
                .borrow_mut()
                .insert(method_name.to_string());

            // Parse content and find the method
            let parse_result = ruby_prism::parse(content);
            let node = parse_result.node();

            for (line, entry_id) in methods_needing_inference {
                if let Some(def_node) = self.find_def_node_at_line(&node, line, content) {
                    // Create a new inferrer WITHOUT content to prevent deep recursion
                    let simple_inferrer = ReturnTypeInferrer::new(self.index.clone());
                    if let Some(inferred_ty) = simple_inferrer.infer_return_type(content, &def_node)
                    {
                        if inferred_ty != RubyType::Unknown {
                            // Cache the result in the index
                            self.index
                                .lock()
                                .update_method_return_type(entry_id, inferred_ty.clone());
                            return_types.push(inferred_ty);
                        }
                    }
                }
            }

            // Remove from in-progress set
            self.inference_in_progress.borrow_mut().remove(method_name);

            if !return_types.is_empty() {
                return Some(RubyType::union(return_types));
            }
        }

        None
    }

    /// Find a DefNode at the given line in the AST (helper for on-demand inference)
    fn find_def_node_at_line<'a>(
        &self,
        node: &ruby_prism::Node<'a>,
        target_line: u32,
        content: &[u8],
    ) -> Option<ruby_prism::DefNode<'a>> {
        if let Some(def_node) = node.as_def_node() {
            let offset = def_node.location().start_offset();
            let line = content[..offset].iter().filter(|&&b| b == b'\n').count() as u32;
            if line == target_line {
                return Some(def_node);
            }
        }

        // Recurse into child nodes
        if let Some(program) = node.as_program_node() {
            for stmt in program.statements().body().iter() {
                if let Some(found) = self.find_def_node_at_line(&stmt, target_line, content) {
                    return Some(found);
                }
            }
        }

        if let Some(class_node) = node.as_class_node() {
            if let Some(body) = class_node.body() {
                if let Some(stmts) = body.as_statements_node() {
                    for stmt in stmts.body().iter() {
                        if let Some(found) = self.find_def_node_at_line(&stmt, target_line, content)
                        {
                            return Some(found);
                        }
                    }
                }
            }
        }

        if let Some(module_node) = node.as_module_node() {
            if let Some(body) = module_node.body() {
                if let Some(stmts) = body.as_statements_node() {
                    for stmt in stmts.body().iter() {
                        if let Some(found) = self.find_def_node_at_line(&stmt, target_line, content)
                        {
                            return Some(found);
                        }
                    }
                }
            }
        }

        if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                if let Some(found) = self.find_def_node_at_line(&stmt, target_line, content) {
                    return Some(found);
                }
            }
        }

        None
    }

    /// Look up a local variable's type from the index.
    /// This is a fallback when the CFG doesn't have type information.
    fn lookup_local_variable_type(&self, var_name: &str) -> Option<RubyType> {
        let index = self.index.lock();

        for (fqn, entries) in index.definitions() {
            if let FullyQualifiedName::LocalVariable(name, _) = fqn {
                if name == var_name {
                    for entry in entries {
                        if let EntryKind::LocalVariable(data) = &entry.kind {
                            let r#type = &data.r#type;
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

    /// Collect all ReturnNodes from a method body for re-inference
    fn collect_return_nodes<'a>(&self, method: &'a DefNode<'a>) -> Vec<ReturnNode<'a>> {
        let mut return_nodes = Vec::new();
        if let Some(body) = method.body() {
            self.collect_return_nodes_from_node(&body, &mut return_nodes);
        }
        return_nodes
    }

    /// Recursively collect ReturnNodes from an AST node
    fn collect_return_nodes_from_node<'a>(
        &self,
        node: &Node<'a>,
        results: &mut Vec<ReturnNode<'a>>,
    ) {
        if let Some(ret_node) = node.as_return_node() {
            results.push(ret_node);
            return;
        }

        // Recurse into statements nodes
        if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                self.collect_return_nodes_from_node(&stmt, results);
            }
            return;
        }

        // Recurse into if nodes
        if let Some(if_node) = node.as_if_node() {
            if let Some(stmts) = if_node.statements() {
                self.collect_return_nodes_from_node(&stmts.as_node(), results);
            }
            if let Some(subsequent) = if_node.subsequent() {
                self.collect_return_nodes_from_node(&subsequent, results);
            }
            return;
        }

        // Recurse into else nodes
        if let Some(else_node) = node.as_else_node() {
            if let Some(stmts) = else_node.statements() {
                self.collect_return_nodes_from_node(&stmts.as_node(), results);
            }
            return;
        }

        // Recurse into case nodes
        if let Some(case_node) = node.as_case_node() {
            for condition in case_node.conditions().iter() {
                if let Some(when_node) = condition.as_when_node() {
                    if let Some(stmts) = when_node.statements() {
                        self.collect_return_nodes_from_node(&stmts.as_node(), results);
                    }
                }
            }
            if let Some(else_clause) = case_node.else_clause() {
                self.collect_return_nodes_from_node(&else_clause.as_node(), results);
            }
            return;
        }

        // Recurse into begin nodes
        if let Some(begin_node) = node.as_begin_node() {
            if let Some(stmts) = begin_node.statements() {
                self.collect_return_nodes_from_node(&stmts.as_node(), results);
            }
            if let Some(rescue) = begin_node.rescue_clause() {
                self.collect_return_nodes_from_rescue(&rescue, results);
            }
            if let Some(else_clause) = begin_node.else_clause() {
                self.collect_return_nodes_from_node(&else_clause.as_node(), results);
            }
            if let Some(ensure) = begin_node.ensure_clause() {
                if let Some(stmts) = ensure.statements() {
                    self.collect_return_nodes_from_node(&stmts.as_node(), results);
                }
            }
            return;
        }
    }

    /// Recursively collect ReturnNodes from rescue chains
    fn collect_return_nodes_from_rescue<'a>(
        &self,
        rescue: &RescueNode<'a>,
        results: &mut Vec<ReturnNode<'a>>,
    ) {
        if let Some(stmts) = rescue.statements() {
            self.collect_return_nodes_from_node(&stmts.as_node(), results);
        }
        if let Some(subsequent) = rescue.subsequent() {
            self.collect_return_nodes_from_rescue(&subsequent, results);
        }
    }

    /// Get class name for RBS lookup from a RubyType
    fn get_class_name_for_rbs(&self, ruby_type: &RubyType) -> Option<String> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
                if let FullyQualifiedName::Constant(parts) = fqn {
                    Some(
                        parts
                            .iter()
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join("::"),
                    )
                } else {
                    None
                }
            }
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
            "Array" => RubyType::Array(vec![RubyType::Unknown]),
            "Hash" => RubyType::Hash(vec![RubyType::Unknown], vec![RubyType::Unknown]),
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
