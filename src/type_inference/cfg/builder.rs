//! CFG Builder - constructs Control Flow Graphs from Ruby AST.
//!
//! This module converts Ruby AST (from Prism parser) into a Control Flow Graph
//! suitable for dataflow analysis and type narrowing.

use ruby_prism::*;

use crate::type_inference::literal_analyzer::LiteralAnalyzer;
use crate::type_inference::ruby_type::RubyType;

use super::graph::{BlockId, BlockLocation, CfgEdge, ControlFlowGraph, Statement};
use super::guards::TypeGuard;

/// Builds a Control Flow Graph from Ruby AST
pub struct CfgBuilder<'a> {
    source: &'a [u8],
    cfg: ControlFlowGraph,
    current_block: Option<BlockId>,
    literal_analyzer: LiteralAnalyzer,
}

impl<'a> CfgBuilder<'a> {
    pub fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            cfg: ControlFlowGraph::new(),
            current_block: None,
            literal_analyzer: LiteralAnalyzer::new(),
        }
    }

    /// Build CFG from a method definition
    pub fn build_from_method(mut self, method: &DefNode) -> ControlFlowGraph {
        // Create entry block
        let entry = self.cfg.create_block();
        self.cfg.entry = entry;
        self.current_block = Some(entry);

        // Set entry block location
        let location = self.node_location(method);
        if let Some(block) = self.cfg.get_block_mut(entry) {
            block.location = location;
        }

        // Extract parameters
        if let Some(params) = method.parameters() {
            self.process_parameters(&params);
        }

        // Process method body
        if let Some(body) = method.body() {
            self.process_node(&body);
        }

        // Mark current block as exit if not already terminated
        if let Some(current) = self.current_block {
            self.cfg.mark_exit(current);
        }

        self.cfg
    }

    /// Build CFG from a block/lambda body
    pub fn build_from_block(mut self, body: &Node) -> ControlFlowGraph {
        let entry = self.cfg.create_block();
        self.cfg.entry = entry;
        self.current_block = Some(entry);

        self.process_node(body);

        if let Some(current) = self.current_block {
            self.cfg.mark_exit(current);
        }

        self.cfg
    }

    /// Build CFG from top-level statements (outside any method)
    pub fn build_from_statements(mut self, statements: &StatementsNode) -> ControlFlowGraph {
        let entry = self.cfg.create_block();
        self.cfg.entry = entry;
        self.current_block = Some(entry);

        // Set entry block location from statements
        let loc = statements.location();
        let (start_line, start_col) = self.offset_to_line_col(loc.start_offset());
        let (end_line, end_col) = self.offset_to_line_col(loc.end_offset());
        if let Some(block) = self.cfg.get_block_mut(entry) {
            block.location = BlockLocation::new(
                start_line,
                start_col,
                end_line,
                end_col,
                loc.start_offset(),
                loc.end_offset(),
            );
        }

        // Process each statement
        for stmt in statements.body().iter() {
            self.process_node(&stmt);
        }

        if let Some(current) = self.current_block {
            self.cfg.mark_exit(current);
        }

        self.cfg
    }

    /// Get node location as BlockLocation
    fn node_location(&self, node: &DefNode) -> BlockLocation {
        let loc = node.location();
        // ruby_prism Location uses start_offset/end_offset
        // We need to calculate line/column from offsets
        let (start_line, start_col) = self.offset_to_line_col(loc.start_offset());
        let (end_line, end_col) = self.offset_to_line_col(loc.end_offset());
        BlockLocation::new(
            start_line,
            start_col,
            end_line,
            end_col,
            loc.start_offset(),
            loc.end_offset(),
        )
    }

    /// Convert byte offset to line and column
    fn offset_to_line_col(&self, offset: usize) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 0u32;
        for (i, &byte) in self.source.iter().enumerate() {
            if i >= offset {
                break;
            }
            if byte == b'\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Process method parameters
    fn process_parameters(&mut self, params: &ParametersNode) {
        // Required parameters
        for param in params.requireds().iter() {
            if let Some(req) = param.as_required_parameter_node() {
                let name = String::from_utf8_lossy(req.name().as_slice()).to_string();
                self.cfg.parameters.push((name, RubyType::Unknown));
            }
        }

        // Optional parameters
        for param in params.optionals().iter() {
            if let Some(opt) = param.as_optional_parameter_node() {
                let name = String::from_utf8_lossy(opt.name().as_slice()).to_string();
                // Could infer type from default value
                let default_type = self.literal_analyzer.analyze_literal(&opt.value());
                self.cfg
                    .parameters
                    .push((name, default_type.unwrap_or(RubyType::Unknown)));
            }
        }

        // Rest parameter (*args)
        if let Some(rest) = params.rest() {
            if let Some(rest_param) = rest.as_rest_parameter_node() {
                if let Some(name) = rest_param.name() {
                    let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
                    self.cfg
                        .parameters
                        .push((name_str, RubyType::Array(vec![RubyType::Any])));
                }
            }
        }

        // Keyword parameters
        for param in params.keywords().iter() {
            if let Some(kw) = param.as_required_keyword_parameter_node() {
                let name = String::from_utf8_lossy(kw.name().as_slice()).to_string();
                // Remove trailing colon from keyword name
                let name = name.trim_end_matches(':').to_string();
                self.cfg.parameters.push((name, RubyType::Unknown));
            } else if let Some(kw) = param.as_optional_keyword_parameter_node() {
                let name = String::from_utf8_lossy(kw.name().as_slice()).to_string();
                let name = name.trim_end_matches(':').to_string();
                // value() returns Node directly, not Option<Node>
                let default_type = self.literal_analyzer.analyze_literal(&kw.value());
                self.cfg
                    .parameters
                    .push((name, default_type.unwrap_or(RubyType::Unknown)));
            }
        }

        // Keyword rest (**kwargs)
        if let Some(kw_rest) = params.keyword_rest() {
            if let Some(kw_rest_param) = kw_rest.as_keyword_rest_parameter_node() {
                if let Some(name) = kw_rest_param.name() {
                    let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
                    self.cfg.parameters.push((
                        name_str,
                        RubyType::Hash(vec![RubyType::symbol()], vec![RubyType::Any]),
                    ));
                }
            }
        }

        // Block parameter (&block)
        if let Some(block_param) = params.block() {
            if let Some(name) = block_param.name() {
                let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
                // Blocks are Proc or nil
                self.cfg.parameters.push((
                    name_str,
                    RubyType::Union(vec![RubyType::class("Proc"), RubyType::nil_class()]),
                ));
            }
        }
    }

    /// Add a statement to the current block
    fn add_statement(&mut self, stmt: Statement) {
        if let Some(block_id) = self.current_block {
            if let Some(block) = self.cfg.get_block_mut(block_id) {
                block.statements.push(stmt);
            }
        }
    }

    /// Process a node and add to CFG
    fn process_node(&mut self, node: &Node) {
        // Early return if no current block (terminated by return)
        if self.current_block.is_none() {
            return;
        }

        if let Some(if_node) = node.as_if_node() {
            self.process_if(&if_node);
        } else if let Some(unless_node) = node.as_unless_node() {
            self.process_unless(&unless_node);
        } else if let Some(case_node) = node.as_case_node() {
            self.process_case(&case_node);
        } else if let Some(case_match) = node.as_case_match_node() {
            self.process_case_match(&case_match);
        } else if let Some(while_node) = node.as_while_node() {
            self.process_while(&while_node);
        } else if let Some(until_node) = node.as_until_node() {
            self.process_until(&until_node);
        } else if let Some(for_node) = node.as_for_node() {
            self.process_for(&for_node);
        } else if let Some(return_node) = node.as_return_node() {
            self.process_return(&return_node);
        } else if let Some(begin_node) = node.as_begin_node() {
            self.process_begin(&begin_node);
        } else if let Some(assign) = node.as_local_variable_write_node() {
            self.process_local_variable_write(&assign);
        } else if let Some(assign) = node.as_instance_variable_write_node() {
            self.process_instance_variable_write(&assign);
        } else if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                self.process_node(&stmt);
            }
        } else if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                self.process_node(&body);
            }
        } else {
            // Generic expression (includes and/or nodes)
            self.add_expression_statement(node);
        }
    }

    /// Add a generic expression statement
    fn add_expression_statement(&mut self, node: &Node) {
        let loc = node.location();
        self.add_statement(Statement::expression(loc.start_offset(), loc.end_offset()));
    }

    /// Process if statement
    fn process_if(&mut self, if_node: &IfNode) {
        let condition_block = match self.current_block {
            Some(b) => b,
            None => return,
        };

        // Detect type guard from condition
        let guard: Option<TypeGuard> = self.detect_type_guard_from_node(&if_node.predicate());

        // Create then-block
        let then_block = match &guard {
            Some(g) => self.cfg.create_block_with_guard(g.clone()),
            None => self.cfg.create_block(),
        };

        // Create else-block
        let else_block = match &guard {
            Some(g) => self.cfg.create_block_with_guard(g.negate()),
            None => self.cfg.create_block(),
        };

        // Create merge block
        let merge_block = self.cfg.create_block();

        // Add edges from condition to branches
        let true_guard = guard.clone().unwrap_or(TypeGuard::Unknown);
        let false_guard = guard.map(|g| g.negate()).unwrap_or(TypeGuard::Unknown);

        self.cfg.add_edge(CfgEdge::conditional_true(
            condition_block,
            then_block,
            true_guard,
        ));
        self.cfg.add_edge(CfgEdge::conditional_false(
            condition_block,
            else_block,
            false_guard,
        ));

        // Process then-branch
        self.current_block = Some(then_block);
        if let Some(stmts) = if_node.statements() {
            self.process_node(&stmts.as_node());
        }
        // Connect then-block to merge (if not terminated)
        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, merge_block));
        }

        // Process else-branch
        self.current_block = Some(else_block);
        if let Some(else_clause) = if_node.subsequent() {
            self.process_else_clause(&else_clause);
        }
        // Connect else-block to merge (if not terminated)
        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, merge_block));
        }

        // Continue with merge block
        self.current_block = Some(merge_block);
    }

    /// Process else clause (handles elsif chains)
    fn process_else_clause(&mut self, node: &Node) {
        if let Some(else_node) = node.as_else_node() {
            if let Some(stmts) = else_node.statements() {
                self.process_node(&stmts.as_node());
            }
        } else if let Some(elsif_node) = node.as_if_node() {
            // elsif is a nested if
            self.process_if(&elsif_node);
        }
    }

    /// Process unless statement
    fn process_unless(&mut self, unless_node: &UnlessNode) {
        let condition_block = match self.current_block {
            Some(b) => b,
            None => return,
        };

        let guard: Option<TypeGuard> = self.detect_type_guard_from_node(&unless_node.predicate());

        // For unless, the "then" block is when condition is FALSE
        let then_block = match &guard {
            Some(g) => self.cfg.create_block_with_guard(g.negate()),
            None => self.cfg.create_block(),
        };

        let else_block = match &guard {
            Some(g) => self.cfg.create_block_with_guard(g.clone()),
            None => self.cfg.create_block(),
        };

        let merge_block = self.cfg.create_block();

        // Edges are inverted from if
        let true_guard = guard.clone().unwrap_or(TypeGuard::Unknown);
        let false_guard = guard.map(|g| g.negate()).unwrap_or(TypeGuard::Unknown);

        self.cfg.add_edge(CfgEdge::conditional_false(
            condition_block,
            then_block,
            false_guard,
        ));
        self.cfg.add_edge(CfgEdge::conditional_true(
            condition_block,
            else_block,
            true_guard,
        ));

        // Process then-branch (when condition is false)
        self.current_block = Some(then_block);
        if let Some(stmts) = unless_node.statements() {
            self.process_node(&stmts.as_node());
        }
        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, merge_block));
        }

        // Process else-branch (when condition is true)
        self.current_block = Some(else_block);
        if let Some(else_clause) = unless_node.else_clause() {
            if let Some(stmts) = else_clause.statements() {
                self.process_node(&stmts.as_node());
            }
        }
        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, merge_block));
        }

        self.current_block = Some(merge_block);
    }

    /// Process case statement
    fn process_case(&mut self, case_node: &CaseNode) {
        let condition_block = match self.current_block {
            Some(b) => b,
            None => return,
        };

        let merge_block = self.cfg.create_block();

        // Get the case predicate variable
        let case_var = case_node
            .predicate()
            .and_then(|p| self.extract_variable_name(&p));

        // Process each when clause
        let mut previous_block = condition_block;
        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                // Extract pattern type from when conditions
                let pattern_type = self.extract_when_pattern_type(&when_node);

                let guard = if let (Some(ref var), Some(ref ty)) = (&case_var, &pattern_type) {
                    TypeGuard::case_match(var.clone(), ty.clone())
                } else {
                    TypeGuard::Unknown
                };

                let when_block = self.cfg.create_block_with_guard(guard.clone());
                let not_matched_block = self.cfg.create_block_with_guard(guard.negate());

                // Edge: previous -> when (matched)
                self.cfg.add_edge(CfgEdge::conditional_true(
                    previous_block,
                    when_block,
                    guard.clone(),
                ));

                // Edge: previous -> not_matched (not matched)
                self.cfg.add_edge(CfgEdge::conditional_false(
                    previous_block,
                    not_matched_block,
                    guard.negate(),
                ));

                // Process when body
                self.current_block = Some(when_block);
                if let Some(stmts) = when_node.statements() {
                    self.process_node(&stmts.as_node());
                }
                if let Some(current) = self.current_block {
                    self.cfg
                        .add_edge(CfgEdge::unconditional(current, merge_block));
                }

                previous_block = not_matched_block;
            }
        }

        // Process else clause
        if let Some(else_clause) = case_node.else_clause() {
            self.current_block = Some(previous_block);
            if let Some(stmts) = else_clause.statements() {
                self.process_node(&stmts.as_node());
            }
            if let Some(current) = self.current_block {
                self.cfg
                    .add_edge(CfgEdge::unconditional(current, merge_block));
            }
        } else {
            // No else - connect last not-matched to merge
            self.cfg
                .add_edge(CfgEdge::unconditional(previous_block, merge_block));
        }

        self.current_block = Some(merge_block);
    }

    /// Process case/in pattern matching (Ruby 3.0+)
    fn process_case_match(&mut self, case_match: &CaseMatchNode) {
        // Similar to case but with pattern matching
        // For now, treat as a single block (can be enhanced later)
        let condition_block = match self.current_block {
            Some(b) => b,
            None => return,
        };

        let merge_block = self.cfg.create_block();

        // Process each in clause
        for condition in case_match.conditions().iter() {
            let in_block = self.cfg.create_block();
            self.cfg
                .add_edge(CfgEdge::unconditional(condition_block, in_block));

            self.current_block = Some(in_block);
            if let Some(in_node) = condition.as_in_node() {
                if let Some(stmts) = in_node.statements() {
                    self.process_node(&stmts.as_node());
                }
            }
            if let Some(current) = self.current_block {
                self.cfg
                    .add_edge(CfgEdge::unconditional(current, merge_block));
            }
        }

        // Process else
        if let Some(else_clause) = case_match.else_clause() {
            let else_block = self.cfg.create_block();
            self.cfg
                .add_edge(CfgEdge::unconditional(condition_block, else_block));

            self.current_block = Some(else_block);
            if let Some(stmts) = else_clause.statements() {
                self.process_node(&stmts.as_node());
            }
            if let Some(current) = self.current_block {
                self.cfg
                    .add_edge(CfgEdge::unconditional(current, merge_block));
            }
        }

        self.current_block = Some(merge_block);
    }

    /// Process while loop
    fn process_while(&mut self, while_node: &WhileNode) {
        let current = match self.current_block {
            Some(b) => b,
            None => return,
        };

        let header_block = self.cfg.create_block();
        let body_block = self.cfg.create_block();
        let exit_block = self.cfg.create_block();

        // Detect guard from condition
        let guard = self.detect_type_guard_from_node(&while_node.predicate());

        // Current -> header
        self.cfg
            .add_edge(CfgEdge::unconditional(current, header_block));

        // Header -> body (condition true)
        let true_guard = guard.clone().unwrap_or(TypeGuard::Unknown);
        self.cfg.add_edge(CfgEdge::conditional_true(
            header_block,
            body_block,
            true_guard,
        ));

        // Header -> exit (condition false)
        let false_guard = guard.map(|g| g.negate()).unwrap_or(TypeGuard::Unknown);
        self.cfg.add_edge(CfgEdge::conditional_false(
            header_block,
            exit_block,
            false_guard,
        ));

        // Process body
        self.current_block = Some(body_block);
        if let Some(stmts) = while_node.statements() {
            self.process_node(&stmts.as_node());
        }

        // Body -> header (loop back)
        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, header_block));
        }

        self.current_block = Some(exit_block);
    }

    /// Process until loop
    fn process_until(&mut self, until_node: &UntilNode) {
        let current = match self.current_block {
            Some(b) => b,
            None => return,
        };

        let header_block = self.cfg.create_block();
        let body_block = self.cfg.create_block();
        let exit_block = self.cfg.create_block();

        // Until is while with inverted condition
        let guard = self.detect_type_guard_from_node(&until_node.predicate());

        self.cfg
            .add_edge(CfgEdge::unconditional(current, header_block));

        // Header -> body (condition false - until loops while condition is false)
        let false_guard = guard
            .clone()
            .map(|g| g.negate())
            .unwrap_or(TypeGuard::Unknown);
        self.cfg.add_edge(CfgEdge::conditional_false(
            header_block,
            body_block,
            false_guard,
        ));

        // Header -> exit (condition true)
        let true_guard = guard.unwrap_or(TypeGuard::Unknown);
        self.cfg.add_edge(CfgEdge::conditional_true(
            header_block,
            exit_block,
            true_guard,
        ));

        self.current_block = Some(body_block);
        if let Some(stmts) = until_node.statements() {
            self.process_node(&stmts.as_node());
        }

        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, header_block));
        }

        self.current_block = Some(exit_block);
    }

    /// Process for loop
    fn process_for(&mut self, for_node: &ForNode) {
        let current = match self.current_block {
            Some(b) => b,
            None => return,
        };

        let header_block = self.cfg.create_block();
        let body_block = self.cfg.create_block();
        let exit_block = self.cfg.create_block();

        self.cfg
            .add_edge(CfgEdge::unconditional(current, header_block));
        self.cfg.add_edge(CfgEdge::conditional_true(
            header_block,
            body_block,
            TypeGuard::Unknown,
        ));
        self.cfg.add_edge(CfgEdge::conditional_false(
            header_block,
            exit_block,
            TypeGuard::Unknown,
        ));

        self.current_block = Some(body_block);
        if let Some(stmts) = for_node.statements() {
            self.process_node(&stmts.as_node());
        }

        if let Some(current) = self.current_block {
            self.cfg
                .add_edge(CfgEdge::unconditional(current, header_block));
        }

        self.current_block = Some(exit_block);
    }

    /// Process return statement
    fn process_return(&mut self, return_node: &ReturnNode) {
        let value_type = return_node
            .arguments()
            .and_then(|args| args.arguments().iter().next())
            .and_then(|arg| self.literal_analyzer.analyze_literal(&arg));

        let loc = return_node.location();
        self.add_statement(Statement::return_stmt(
            loc.start_offset(),
            loc.end_offset(),
            value_type,
        ));

        // Mark current block as exit and terminate
        if let Some(block) = self.current_block {
            self.cfg.mark_exit(block);
        }
        self.current_block = None;
    }

    /// Process begin/rescue/ensure
    fn process_begin(&mut self, begin_node: &BeginNode) {
        // Process main body
        if let Some(stmts) = begin_node.statements() {
            self.process_node(&stmts.as_node());
        }

        // Process rescue clauses
        if let Some(rescue) = begin_node.rescue_clause() {
            self.process_rescue_chain(&rescue);
        }

        // Process ensure clause
        if let Some(ensure) = begin_node.ensure_clause() {
            if let Some(stmts) = ensure.statements() {
                self.process_node(&stmts.as_node());
            }
        }
    }

    /// Process rescue clause chain
    fn process_rescue_chain(&mut self, rescue: &RescueNode) {
        let try_block = self.current_block;

        // Create rescue block
        let rescue_block = self.cfg.create_block();

        // Add exception edge from try to rescue
        if let Some(try_b) = try_block {
            self.cfg.add_edge(CfgEdge::exception(try_b, rescue_block));
        }

        self.current_block = Some(rescue_block);
        if let Some(stmts) = rescue.statements() {
            self.process_node(&stmts.as_node());
        }

        // Process subsequent rescue clauses
        if let Some(subsequent) = rescue.subsequent() {
            self.process_rescue_chain(&subsequent);
        }
    }

    /// Process local variable write
    fn process_local_variable_write(&mut self, assign: &LocalVariableWriteNode) {
        let name = String::from_utf8_lossy(assign.name().as_slice()).to_string();
        let loc = assign.location();

        // Check if the value is a local variable read (variable-to-variable assignment)
        if let Some(var_read) = assign.value().as_local_variable_read_node() {
            let source_var = String::from_utf8_lossy(var_read.name().as_slice()).to_string();
            self.add_statement(Statement::assignment_from_variable(
                loc.start_offset(),
                loc.end_offset(),
                name,
                source_var,
            ));
            return;
        }

        // Otherwise, try to analyze as a literal
        let value_type = self.literal_analyzer.analyze_literal(&assign.value());

        self.add_statement(Statement::assignment(
            loc.start_offset(),
            loc.end_offset(),
            name,
            value_type,
        ));
    }

    /// Process instance variable write
    fn process_instance_variable_write(&mut self, assign: &InstanceVariableWriteNode) {
        let name = String::from_utf8_lossy(assign.name().as_slice()).to_string();
        let value_type = self.literal_analyzer.analyze_literal(&assign.value());

        let loc = assign.location();
        self.add_statement(Statement::assignment(
            loc.start_offset(),
            loc.end_offset(),
            name,
            value_type,
        ));
    }

    /// Detect type guard from a condition expression
    fn detect_type_guard_from_node(&self, node: &Node) -> Option<TypeGuard> {
        // Check for method call guards
        if let Some(call) = node.as_call_node() {
            return self.detect_call_guard(&call);
        }

        // Check for && (and)
        if let Some(and_node) = node.as_and_node() {
            let left = self.detect_type_guard_from_node(&and_node.left());
            let right = self.detect_type_guard_from_node(&and_node.right());
            return match (left, right) {
                (Some(l), Some(r)) => Some(TypeGuard::and(vec![l, r])),
                (Some(g), None) | (None, Some(g)) => Some(g),
                _ => None,
            };
        }

        // Check for || (or)
        if let Some(or_node) = node.as_or_node() {
            let left = self.detect_type_guard_from_node(&or_node.left());
            let right = self.detect_type_guard_from_node(&or_node.right());
            return match (left, right) {
                (Some(l), Some(r)) => Some(TypeGuard::or(vec![l, r])),
                (Some(g), None) | (None, Some(g)) => Some(g),
                _ => None,
            };
        }

        // Simple variable in condition means truthy check
        if let Some(var_name) = self.extract_variable_name(node) {
            return Some(TypeGuard::not_nil(var_name));
        }

        None
    }

    /// Detect type guard from a method call
    fn detect_call_guard(&self, call: &CallNode) -> Option<TypeGuard> {
        let method = String::from_utf8_lossy(call.name().as_slice()).to_string();

        // Handle negation: !x.nil? etc.
        if method == "!" {
            if let Some(receiver) = call.receiver() {
                if let Some(inner_guard) = self.detect_type_guard_from_node(&receiver) {
                    return Some(inner_guard.negate());
                }
            }
            return None;
        }

        let receiver = call.receiver()?;
        let var_name = self.extract_variable_name(&receiver)?;

        match method.as_str() {
            "is_a?" | "kind_of?" | "instance_of?" => {
                let type_arg = self.extract_type_from_arguments(call.arguments())?;
                Some(TypeGuard::is_a(var_name, type_arg))
            }
            "nil?" => Some(TypeGuard::is_nil(var_name)),
            "respond_to?" => {
                let method_name = self.extract_symbol_from_arguments(call.arguments())?;
                Some(TypeGuard::responds_to(var_name, method_name))
            }
            _ => None,
        }
    }

    /// Extract variable name from a node
    fn extract_variable_name(&self, node: &Node) -> Option<String> {
        if let Some(local) = node.as_local_variable_read_node() {
            return Some(String::from_utf8_lossy(local.name().as_slice()).to_string());
        }
        if let Some(ivar) = node.as_instance_variable_read_node() {
            return Some(String::from_utf8_lossy(ivar.name().as_slice()).to_string());
        }
        if let Some(cvar) = node.as_class_variable_read_node() {
            return Some(String::from_utf8_lossy(cvar.name().as_slice()).to_string());
        }
        if let Some(gvar) = node.as_global_variable_read_node() {
            return Some(String::from_utf8_lossy(gvar.name().as_slice()).to_string());
        }
        None
    }

    /// Extract type from method arguments (for is_a? etc.)
    fn extract_type_from_arguments(&self, args: Option<ArgumentsNode>) -> Option<RubyType> {
        let args = args?;
        let first_arg = args.arguments().iter().next()?;

        // Handle constant like String, Integer
        if let Some(const_read) = first_arg.as_constant_read_node() {
            let name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
            return Some(self.constant_to_ruby_type(&name));
        }

        // Handle constant path like Foo::Bar
        if let Some(const_path) = first_arg.as_constant_path_node() {
            let name = self.constant_path_to_string(&const_path);
            return Some(RubyType::class(&name));
        }

        None
    }

    /// Extract symbol from method arguments (for respond_to? etc.)
    fn extract_symbol_from_arguments(&self, args: Option<ArgumentsNode>) -> Option<String> {
        let args = args?;
        let first_arg = args.arguments().iter().next()?;

        if let Some(symbol) = first_arg.as_symbol_node() {
            return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
        }

        None
    }

    /// Extract pattern type from when clause
    fn extract_when_pattern_type(&self, when_node: &WhenNode) -> Option<RubyType> {
        // Get first condition
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

    /// Convert constant path to string
    fn constant_path_to_string(&self, path: &ConstantPathNode) -> String {
        let mut parts = Vec::new();

        // Get parent parts
        if let Some(parent) = path.parent() {
            if let Some(const_read) = parent.as_constant_read_node() {
                parts.push(String::from_utf8_lossy(const_read.name().as_slice()).to_string());
            } else if let Some(const_path) = parent.as_constant_path_node() {
                parts.push(self.constant_path_to_string(&const_path));
            }
        }

        // Add current name
        if let Some(name) = path.name() {
            parts.push(String::from_utf8_lossy(name.as_slice()).to_string());
        }

        parts.join("::")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_build_cfg(source: &str) -> ControlFlowGraph {
        let result = ruby_prism::parse(source.as_bytes());
        let node = result.node();
        let program = node.as_program_node().expect("Expected program node");

        // Find the first method definition in the program
        let method = program
            .statements()
            .body()
            .iter()
            .find_map(|node| node.as_def_node())
            .expect("No method found");

        let builder = CfgBuilder::new(source.as_bytes());
        builder.build_from_method(&method)
    }

    #[test]
    fn test_simple_method() {
        let source = r#"
def foo
  x = 1
  y = 2
end
"#;
        let cfg = parse_and_build_cfg(source);

        assert_eq!(cfg.block_count(), 1);
        assert_eq!(cfg.exits.len(), 1);
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
def foo(x)
  if x.nil?
    "nil"
  else
    "not nil"
  end
end
"#;
        let cfg = parse_and_build_cfg(source);

        // Entry + then + else + merge = 4 blocks
        assert!(cfg.block_count() >= 4);
    }

    #[test]
    fn test_unless_statement() {
        let source = r#"
def foo(x)
  unless x.nil?
    "not nil"
  end
end
"#;
        let cfg = parse_and_build_cfg(source);

        assert!(cfg.block_count() >= 3);
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
def foo
  while true
    x = 1
  end
end
"#;
        let cfg = parse_and_build_cfg(source);

        // Entry + header + body + exit = 4 blocks
        assert!(cfg.block_count() >= 4);
    }

    #[test]
    fn test_return_terminates_block() {
        let source = r#"
def foo(x)
  return "early" if x.nil?
  "normal"
end
"#;
        let cfg = parse_and_build_cfg(source);

        // Should have multiple exit blocks
        assert!(!cfg.exits.is_empty());
    }

    #[test]
    fn test_type_guard_detection() {
        let source = r#"
def foo(x)
  if x.is_a?(String)
    x.upcase
  end
end
"#;
        let cfg = parse_and_build_cfg(source);

        // Find the then-block and check it has a type guard
        let mut found_guard = false;
        for block in cfg.blocks.values() {
            for guard in &block.entry_guards {
                if matches!(guard, TypeGuard::IsA { variable, .. } if variable == "x") {
                    found_guard = true;
                }
            }
        }
        assert!(found_guard, "Should detect is_a? type guard");
    }

    #[test]
    fn test_parameters_extracted() {
        let source = r#"
def foo(a, b = 1, *c, d:, e: 2, **f, &g)
end
"#;
        let cfg = parse_and_build_cfg(source);

        // Should have all parameter names
        let param_names: Vec<_> = cfg.parameters.iter().map(|(n, _)| n.as_str()).collect();
        assert!(param_names.contains(&"a"));
        assert!(param_names.contains(&"b"));
        assert!(param_names.contains(&"c"));
        assert!(param_names.contains(&"d"));
        assert!(param_names.contains(&"e"));
        assert!(param_names.contains(&"f"));
        assert!(param_names.contains(&"g"));
    }
}
