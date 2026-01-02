//! Return Type Inference using CFG-based dataflow analysis.
//!
//! This module infers return types from method bodies by:
//! 1. Building a Control Flow Graph (CFG) from the method AST
//! 2. Running dataflow analysis to propagate type narrowing
//! 3. Collecting return types from all exit paths with proper narrowed types

use crate::indexer::entry::{entry_kind::EntryKind, MethodKind};
use crate::indexer::index::FileId;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::cfg::{CfgBuilder, DataflowAnalyzer, StatementKind, TypeState};
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::rbs::get_rbs_method_return_type_as_ruby_type;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use ruby_prism::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use tower_lsp::lsp_types::Url;

/// Infers return types from method bodies using CFG-based dataflow analysis.
/// This properly handles type narrowing in control flow structures like case/when.
pub struct ReturnTypeInferrer {
    index: Index<Unlocked>,
    literal_analyzer: LiteralAnalyzer,
    /// Optional file content for on-demand inference of called methods
    content: Option<Vec<u8>>,
    /// Optional file URI to filter methods to only those in the same file
    file_uri: Option<Url>,
    /// Track methods currently being inferred to prevent infinite recursion
    inference_in_progress: RefCell<HashSet<String>>,
    /// If true, skip cross-file inference (used when inferring during indexing for validation)
    skip_cross_file_inference: bool,
}

impl ReturnTypeInferrer {
    /// Create a new return type inferrer with access to the Ruby index.
    /// Skips cross-file inference (used for validation during indexing).
    pub fn new(index: Index<Unlocked>) -> Self {
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
            content: None,
            file_uri: None,
            inference_in_progress: RefCell::new(HashSet::new()),
            skip_cross_file_inference: true,
        }
    }

    /// Create a new return type inferrer with file content for on-demand inference
    /// When looking up a method that has no return type, it will infer it automatically.
    /// The URI is used to filter methods to only those in the same file.
    pub fn new_with_content(
        index: Index<Unlocked>,
        content: &[u8],
        uri: &tower_lsp::lsp_types::Url,
    ) -> Self {
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
            content: Some(content.to_vec()),
            file_uri: Some(uri.clone()),
            inference_in_progress: RefCell::new(HashSet::new()),
            skip_cross_file_inference: false,
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
                                                // Use entry state of the block containing the return,
                                                // as it has the merged types from all predecessors
                                                let entry_state = results.get_entry_state(*exit_id);
                                                ty = self
                                                    .infer_expression_type(first_arg, entry_state);
                                            }
                                        }
                                        break;
                                    }
                                }
                            }

                            let final_ty = ty.unwrap_or(RubyType::Unknown);
                            return_values.push((final_ty, stmt.start_offset, stmt.end_offset));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Analyze the method body for implicit return, but only if control flow reaches the end
        // (i.e., the last statement is not an explicit return)
        if let Some(body) = method.body() {
            // Check if the last statement is an explicit return
            let ends_with_explicit_return = if let Some(stmts) = body.as_statements_node() {
                stmts
                    .body()
                    .iter()
                    .last()
                    .map(|s| s.as_return_node().is_some())
                    .unwrap_or(false)
            } else {
                body.as_return_node().is_some()
            };

            if !ends_with_explicit_return {
                if let Some((implicit_type, start, end)) =
                    self.infer_implicit_return_with_loc(&body, &cfg, &results)
                {
                    return_values.push((implicit_type, start, end));
                } else if return_values.is_empty() {
                    // Method has a body but we couldn't infer ANY return type
                    // Use Unknown to be honest about what we don't know
                    let loc = body.location();
                    return_values.push((RubyType::Unknown, loc.start_offset(), loc.end_offset()));
                }
            }
            // If ends_with_explicit_return, we already collected the return type above
        } else {
            // Method has no body (empty method) - Ruby returns nil
            let loc = method.name_loc();
            return_values.push((RubyType::nil_class(), loc.start_offset(), loc.end_offset()));
        }

        return_values
    }

    /// Infer the return type from a method definition using CFG analysis.
    /// This properly handles type narrowing in control flow structures.
    pub fn infer_return_type(&self, source: &[u8], method: &DefNode) -> Option<RubyType> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static FAST_PATH_HITS: AtomicU64 = AtomicU64::new(0);
        static CFG_PATH_HITS: AtomicU64 = AtomicU64::new(0);

        // Fast path: try simple inference first (no CFG needed)
        if let Some(ty) = self.try_simple_inference(method) {
            let hits = FAST_PATH_HITS.fetch_add(1, Ordering::Relaxed) + 1;
            if hits % 5000 == 0 {
                log::info!(
                    "Inference: fast_path={}, cfg_path={}",
                    hits,
                    CFG_PATH_HITS.load(Ordering::Relaxed)
                );
            }
            return Some(ty);
        }

        // Full CFG-based inference for complex methods
        let cfg_hits = CFG_PATH_HITS.fetch_add(1, Ordering::Relaxed) + 1;
        if cfg_hits % 5000 == 0 {
            log::info!(
                "Inference: fast_path={}, cfg_path={}",
                FAST_PATH_HITS.load(Ordering::Relaxed),
                cfg_hits
            );
        }

        let values = self.infer_return_values(source, method);
        if values.is_empty() {
            None
        } else {
            let types: Vec<RubyType> = values.into_iter().map(|(t, _, _)| t).collect();
            Some(RubyType::union(types))
        }
    }

    /// Fast path inference for simple methods without building CFG.
    /// Handles methods that return literals, simple expressions, or have predictable patterns.
    fn try_simple_inference(&self, method: &DefNode) -> Option<RubyType> {
        let body = method.body()?;

        if let Some(stmts) = body.as_statements_node() {
            let stmt_list: Vec<_> = stmts.body().iter().collect();

            // Empty body - returns nil
            if stmt_list.is_empty() {
                return Some(RubyType::nil_class());
            }

            // Check last statement for implicit return
            let last = stmt_list.last()?;

            // Explicit return statement
            if let Some(ret) = last.as_return_node() {
                if let Some(args) = ret.arguments() {
                    let args_list: Vec<_> = args.arguments().iter().collect();
                    if args_list.len() == 1 {
                        return self.infer_simple_expression(&args_list[0]);
                    }
                }
                return Some(RubyType::nil_class());
            }

            // Implicit return - try to infer from last expression
            return self.infer_simple_expression(last);
        }

        // Direct expression body (not wrapped in statements)
        self.infer_simple_expression(&body)
    }

    /// Infer type from a simple expression without CFG.
    /// Returns None if the expression is too complex.
    fn infer_simple_expression(&self, node: &Node) -> Option<RubyType> {
        // Literals
        if let Some(ty) = self.literal_analyzer.analyze_literal(node) {
            return Some(ty);
        }

        // Constant read (e.g., `SomeClass`) - returns the class reference
        if let Some(const_read) = node.as_constant_read_node() {
            let name = std::str::from_utf8(const_read.name().as_slice()).unwrap_or("");
            if let Ok(fqn) = FullyQualifiedName::try_from(name) {
                return Some(RubyType::ClassReference(fqn));
            }
        }

        // Constant path (e.g., `Foo::Bar`) - returns the class reference
        if node.as_constant_path_node().is_some() {
            // For now, just return Unknown for constant paths - they need proper resolution
            // The full CFG path handles this better
            return None;
        }

        // Interpolated string always returns String
        if node.as_interpolated_string_node().is_some() {
            return Some(RubyType::string());
        }

        // Interpolated symbol always returns Symbol
        if node.as_interpolated_symbol_node().is_some() {
            return Some(RubyType::symbol());
        }

        // Self - we don't know the type without context
        if node.as_self_node().is_some() {
            return None; // Need context to know self's type
        }

        // Instance variable (we don't know the type without context)
        if node.as_instance_variable_read_node().is_some() {
            return None; // Need CFG for this
        }

        // Local variable (we don't know the type without CFG)
        if node.as_local_variable_read_node().is_some() {
            return None; // Need CFG for this
        }

        // Method call - too complex, need CFG
        if node.as_call_node().is_some() {
            return None;
        }

        // If/case expressions - too complex, need CFG
        if node.as_if_node().is_some() || node.as_case_node().is_some() {
            return None;
        }

        None
    }

    fn infer_implicit_return_with_loc(
        &self,
        body: &Node,
        cfg: &crate::inferrer::cfg::ControlFlowGraph,
        results: &crate::inferrer::cfg::DataflowResults,
    ) -> Option<(RubyType, usize, usize)> {
        // Get the last statement's type
        if let Some(statements) = body.as_statements_node() {
            let stmts: Vec<_> = statements.body().iter().collect();
            if let Some(last_stmt) = stmts.last() {
                let start = last_stmt.location().start_offset();
                let end = last_stmt.location().end_offset();

                // For implicit returns, we need to collect types from ALL exit blocks.
                // This is important for variables that are modified in different branches
                // (e.g., `flag = false; if cond; flag = true; end; flag` should return
                // TrueClass | FalseClass).
                //
                // Each exit block's exit_state contains the merged types from all paths
                // leading to that block, so we union the types from all exit blocks.
                let mut types_from_exits = Vec::new();
                for exit_id in &cfg.exits {
                    if let Some(exit_state) = results.get_exit_state(*exit_id) {
                        if let Some(ty) = self.infer_expression_type(last_stmt, Some(exit_state)) {
                            types_from_exits.push(ty);
                        }
                    }
                }

                if !types_from_exits.is_empty() {
                    return Some((RubyType::union(types_from_exits), start, end));
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

        // Check if there's an explicit receiver
        let has_receiver = call_node.receiver().is_some();

        // Get receiver type (possibly narrowed)
        let receiver_type = if let Some(receiver) = call_node.receiver() {
            self.get_node_receiver_type(&receiver, state)
        } else {
            None
        };

        if let Some(recv_type) = receiver_type {
            // If receiver type is Unknown, we can't determine the method's return type
            if recv_type == RubyType::Unknown {
                return Some(RubyType::Unknown);
            }
            return self.lookup_method_return_type(&recv_type, &method_name);
        } else if has_receiver {
            // There's a receiver but we couldn't determine its type
            // Return Unknown to indicate we can't infer the type
            // (Don't do a global lookup - that would be incorrect)
            return Some(RubyType::Unknown);
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
    /// If the method has no return type, infer it on-demand (including cross-file).
    fn lookup_method_in_index(
        &self,
        method_name: &str,
        _owner_class: Option<&str>,
    ) -> Option<RubyType> {
        // During bulk indexing, skip expensive index lookups entirely.
        // Return types are being populated in parallel, so lookups would mostly miss anyway.
        // Cross-file inference will happen on-demand when user interacts.
        if self.skip_cross_file_inference {
            return None;
        }

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

        // Methods needing inference, grouped by file for efficient batch processing
        // Same-file methods use current content, other files need to be loaded
        let mut same_file_methods: Vec<(u32, crate::indexer::index::EntryId)> = Vec::new();
        let mut other_file_methods: HashMap<FileId, Vec<(u32, crate::indexer::index::EntryId)>> =
            HashMap::new();

        // Get file_id for the current file if we have a URI
        let current_file_id = self
            .file_uri
            .as_ref()
            .and_then(|uri| index.get_file_id(uri));

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
                                } else if let Some(pos) = data.return_type_position {
                                    // Method has no return type - collect for on-demand inference
                                    let is_same_file = current_file_id
                                        .map(|fid| entry.location.file_id == fid)
                                        .unwrap_or(false);

                                    if is_same_file && self.content.is_some() {
                                        // Same file - use current content
                                        same_file_methods.push((pos.line, *entry_id));
                                    } else {
                                        // Different file - group by file_id for batch loading
                                        other_file_methods
                                            .entry(entry.location.file_id)
                                            .or_default()
                                            .push((pos.line, *entry_id));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Quick return if we found types already
        if !return_types.is_empty() {
            return Some(RubyType::union(return_types));
        }

        // Collect file URLs for cross-file inference before dropping the lock
        let other_file_urls: Vec<(
            tower_lsp::lsp_types::Url,
            Vec<(u32, crate::indexer::index::EntryId)>,
        )> = other_file_methods
            .into_iter()
            .filter_map(|(file_id, methods)| {
                index
                    .get_file_url(file_id)
                    .cloned()
                    .map(|url| (url, methods))
            })
            .collect();

        // Drop the index lock before inference
        drop(index);

        // Mark this method as being inferred (cycle detection)
        self.inference_in_progress
            .borrow_mut()
            .insert(method_name.to_string());

        // 1. Infer same-file methods using current content
        if let Some(content) = &self.content {
            if !same_file_methods.is_empty() {
                let parse_result = ruby_prism::parse(content);
                let node = parse_result.node();

                for (line, entry_id) in same_file_methods.iter() {
                    if let Some(def_node) = self.find_def_node_at_line(&node, *line, content) {
                        let recursive_inferrer = ReturnTypeInferrer {
                            index: self.index.clone(),
                            literal_analyzer: LiteralAnalyzer::new(),
                            content: self.content.clone(),
                            file_uri: self.file_uri.clone(),
                            inference_in_progress: std::cell::RefCell::new(
                                self.inference_in_progress.borrow().clone(),
                            ),
                            skip_cross_file_inference: self.skip_cross_file_inference,
                        };
                        if let Some(inferred_ty) =
                            recursive_inferrer.infer_return_type(content, &def_node)
                        {
                            if inferred_ty != RubyType::Unknown {
                                self.index
                                    .lock()
                                    .update_method_return_type(*entry_id, inferred_ty.clone());
                                return_types.push(inferred_ty);
                            }
                        }
                    }
                }
            }
        }

        // 2. Infer cross-file methods by loading their files
        for (file_url, methods) in other_file_urls {
            // Load file content from disk
            let file_content = match Self::load_file_content(&file_url) {
                Some(content) => content,
                None => continue, // Skip if file can't be read
            };

            let parse_result = ruby_prism::parse(&file_content);
            let node = parse_result.node();

            for (line, entry_id) in methods.iter() {
                if let Some(def_node) =
                    Self::find_def_node_at_line_static(&node, *line, &file_content)
                {
                    // Create inferrer for this file - enables recursive cross-file inference
                    let cross_file_inferrer = ReturnTypeInferrer {
                        index: self.index.clone(),
                        literal_analyzer: LiteralAnalyzer::new(),
                        content: Some(file_content.clone()),
                        file_uri: Some(file_url.clone()),
                        inference_in_progress: std::cell::RefCell::new(
                            self.inference_in_progress.borrow().clone(),
                        ),
                        skip_cross_file_inference: self.skip_cross_file_inference,
                    };
                    if let Some(inferred_ty) =
                        cross_file_inferrer.infer_return_type(&file_content, &def_node)
                    {
                        if inferred_ty != RubyType::Unknown {
                            self.index
                                .lock()
                                .update_method_return_type(*entry_id, inferred_ty.clone());
                            return_types.push(inferred_ty);
                        }
                    }
                }
            }
        }

        // Remove from in-progress set
        self.inference_in_progress.borrow_mut().remove(method_name);

        if !return_types.is_empty() {
            Some(RubyType::union(return_types))
        } else {
            None
        }
    }

    /// Load file content from a URL (synchronous disk read)
    fn load_file_content(url: &tower_lsp::lsp_types::Url) -> Option<Vec<u8>> {
        let path = url.to_file_path().ok()?;
        std::fs::read(&path).ok()
    }

    /// Static version of find_def_node_at_line (doesn't need &self)
    fn find_def_node_at_line_static<'a>(
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
                if let Some(found) = Self::find_def_node_at_line_static(&stmt, target_line, content)
                {
                    return Some(found);
                }
            }
        }

        if let Some(class_node) = node.as_class_node() {
            if let Some(body) = class_node.body() {
                if let Some(stmts) = body.as_statements_node() {
                    for stmt in stmts.body().iter() {
                        if let Some(found) =
                            Self::find_def_node_at_line_static(&stmt, target_line, content)
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
                        if let Some(found) =
                            Self::find_def_node_at_line_static(&stmt, target_line, content)
                        {
                            return Some(found);
                        }
                    }
                }
            }
        }

        if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                if let Some(found) = Self::find_def_node_at_line_static(&stmt, target_line, content)
                {
                    return Some(found);
                }
            }
        }

        if let Some(sclass) = node.as_singleton_class_node() {
            if let Some(body) = sclass.body() {
                if let Some(stmts) = body.as_statements_node() {
                    for stmt in stmts.body().iter() {
                        if let Some(found) =
                            Self::find_def_node_at_line_static(&stmt, target_line, content)
                        {
                            return Some(found);
                        }
                    }
                }
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
    use parking_lot::Mutex;
    use std::sync::Arc;

    fn create_test_index() -> Index<Unlocked> {
        Index::new(Arc::new(
            Mutex::new(crate::indexer::index::RubyIndex::new()),
        ))
    }

    fn create_test_inferrer() -> ReturnTypeInferrer {
        ReturnTypeInferrer::new(create_test_index())
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
        // Empty methods return nil in Ruby - this is deterministic behavior
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
