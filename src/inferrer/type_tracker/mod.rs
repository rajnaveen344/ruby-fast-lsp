//! Simple forward type tracker - replaces CFG with straightforward traversal.
//!
//! This module provides type tracking through Ruby methods using a simple forward
//! traversal of the AST. It handles:
//! - Local variable type tracking
//! - Control flow merging (if/case/while)
//! - Method return type inference
//!
//! Unlike the CFG-based approach, this is a single-pass traversal that creates
//! type snapshots at each statement, with explicit offset ranges showing where
//! each type is valid.

mod narrow;

use crate::analyzer_prism::control_flow;
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use parking_lot::Mutex;
use ruby_analysis_engine::{AnalysisEngine, AnalysisQuery};
use ruby_prism::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

/// Simple forward type tracker with control flow merging.
///
/// Performs a single forward pass through a method's AST, tracking variable
/// types and creating snapshots at each statement. Handles control flow by
/// cloning the environment for branches and merging at join points.
pub struct TypeTracker<'a> {
    /// Current type environment (variable name → type)
    vars: HashMap<String, RubyType>,

    /// Variable types at each offset (for queries)
    /// Key = offset where state was recorded, Value = all variables and their types
    var_types: BTreeMap<usize, HashMap<String, RubyType>>,

    /// Source code (for offset calculations)
    #[allow(dead_code)]
    source: &'a [u8],

    /// Literal analyzer (for static type inference)
    literal_analyzer: LiteralAnalyzer,

    /// Engine for method return type lookups on analysis path
    analysis_engine: Option<Arc<Mutex<AnalysisEngine>>>,

    /// Current URI (for cross-file lookups)
    #[allow(dead_code)]
    uri: &'a Url,

    /// Max loop iterations (to prevent infinite loops)
    max_loop_iterations: usize,

    /// Current class/module context for resolving implicit self
    current_class: Option<FullyQualifiedName>,
}

impl<'a> TypeTracker<'a> {
    /// Create a new type tracker for the given source.
    pub fn new(source: &'a [u8], uri: &'a Url) -> Self {
        Self {
            vars: HashMap::new(),
            var_types: BTreeMap::new(),
            source,
            literal_analyzer: LiteralAnalyzer::new(),
            analysis_engine: None,
            uri,
            max_loop_iterations: 10,
            current_class: None,
        }
    }

    pub fn with_analysis_engine(mut self, analysis_engine: Arc<Mutex<AnalysisEngine>>) -> Self {
        self.analysis_engine = Some(analysis_engine);
        self
    }

    /// Set the current class/module context for resolving implicit self
    pub fn set_current_class(&mut self, fqn: Option<FullyQualifiedName>) {
        self.current_class = fqn;
    }

    /// Get variable types map (for storing in RubyDocument)
    pub fn into_var_types(self) -> BTreeMap<usize, HashMap<String, RubyType>> {
        self.var_types
    }

    /// Record current variable state at an offset
    fn record_state(&mut self, offset: usize) {
        // Only record if there are variables to track
        if !self.vars.is_empty() {
            self.var_types.insert(offset, self.vars.clone());
        }
    }

    /// Track a method definition and return its inferred return type
    ///
    /// This is the main entry point for type tracking. It:
    /// Track a program's top-level statements (outside of methods)
    ///
    /// This tracks variable assignments and control flow at the top level.
    pub fn track_program(&mut self, program: &ProgramNode) -> RubyType {
        let stmts = program.statements();
        self.track_statements(&stmts)
    }

    /// 1. Adds method parameters to the type environment
    /// 2. Tracks the method body, creating snapshots along the way
    /// 3. Returns the inferred return type (type of last expression)
    pub fn track_method(&mut self, method: &DefNode) -> RubyType {
        // Add parameters to environment
        if let Some(params) = method.parameters() {
            self.add_parameters(&params);
        }

        // Track method body
        let return_type = if let Some(body) = method.body() {
            self.track_node(&body)
        } else {
            RubyType::nil_class()
        };

        // Record final state at method end
        if let Some(body) = method.body() {
            let end_offset = body.location().end_offset();
            self.record_state(end_offset);
        }

        return_type
    }

    /// Add method parameters to the type environment
    fn add_parameters(&mut self, _params: &ParametersNode) {
        // TODO: Extract parameter types from YARD/RBS or infer from usage
        // For now, parameters default to Unknown
    }

    /// Track a node and return its type
    ///
    /// This is the main dispatcher that routes to specific tracking methods
    /// based on the node type.
    fn track_node(&mut self, node: &Node) -> RubyType {
        match node {
            // Statements node - track sequence of statements
            _ if node.as_statements_node().is_some() => {
                let stmts = node.as_statements_node().unwrap();
                self.track_statements(&stmts)
            }

            // Local variable assignment
            _ if node.as_local_variable_write_node().is_some() => {
                let write = node.as_local_variable_write_node().unwrap();
                self.track_assignment(&write)
            }

            // If/unless conditionals
            _ if node.as_if_node().is_some() => {
                let if_node = node.as_if_node().unwrap();
                self.track_if(&if_node)
            }

            _ if node.as_unless_node().is_some() => {
                let unless_node = node.as_unless_node().unwrap();
                self.track_unless(&unless_node)
            }

            // Case statement
            _ if node.as_case_node().is_some() => {
                let case_node = node.as_case_node().unwrap();
                self.track_case(&case_node)
            }

            // Loops
            _ if node.as_while_node().is_some() => {
                let while_node = node.as_while_node().unwrap();
                self.track_while(&while_node)
            }

            _ if node.as_until_node().is_some() => {
                let until_node = node.as_until_node().unwrap();
                self.track_until(&until_node)
            }

            // Default: try to infer expression type
            _ => self.infer_expression(node),
        }
    }

    /// Track a sequence of statements and return last expression type
    fn track_statements(&mut self, stmts: &StatementsNode) -> RubyType {
        let mut last_type = RubyType::nil_class();

        for stmt in stmts.body().iter() {
            // Process the statement (this updates self.vars)
            last_type = self.track_node(&stmt);

            // Record state after each statement
            let stmt_end = stmt.location().end_offset();
            self.record_state(stmt_end);
        }

        last_type
    }

    /// Track a local variable assignment
    ///
    /// Infers the type from the value expression and updates the type environment.
    fn track_assignment(&mut self, write: &LocalVariableWriteNode) -> RubyType {
        // Get variable name
        let var_name = String::from_utf8_lossy(write.name().as_slice()).to_string();

        // Infer type from value
        let value = write.value();
        let var_type = self.track_node(&value);

        // Update environment
        self.vars.insert(var_name, var_type.clone());

        // Return the assigned type (assignments return their value in Ruby)
        var_type
    }

    /// Track an if statement with branch merging
    ///
    /// Clones the environment for each branch, tracks them separately,
    /// then merges the results at the join point.
    fn track_if(&mut self, if_node: &IfNode) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = if_node.predicate();
        self.track_node(&predicate);

        let env_before = self.vars.clone();

        // Then branch
        let then_diverges = if_node
            .statements()
            .map(|s| control_flow::diverges(&s.as_node()))
            .unwrap_or(false);
        let then_type = if let Some(statements) = if_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.vars.clone();

        self.vars = env_before.clone();

        // Else branch
        let else_diverges = if_node
            .subsequent()
            .map(|n| control_flow::diverges(&n))
            .unwrap_or(false);
        let else_type = if let Some(subsequent) = if_node.subsequent() {
            match &subsequent {
                _ if subsequent.as_else_node().is_some() => {
                    let else_node = subsequent.as_else_node().unwrap();
                    if let Some(statements) = else_node.statements() {
                        self.track_node(&statements.as_node())
                    } else {
                        RubyType::nil_class()
                    }
                }
                _ if subsequent.as_if_node().is_some() => {
                    let elsif_node = subsequent.as_if_node().unwrap();
                    self.track_if(&elsif_node)
                }
                _ => RubyType::nil_class(),
            }
        } else {
            RubyType::nil_class()
        };
        let else_env = self.vars.clone();

        // Merge envs — diverging branches never reach the join point.
        match (then_diverges, else_diverges) {
            (true, true) => self.vars = env_before,
            (true, false) => {
                // then exited → predicate was false at the join.
                self.vars = else_env;
                narrow::narrow(&mut self.vars, &predicate, false);
            }
            (false, true) => {
                // else exited → predicate was true at the join.
                self.vars = then_env;
                narrow::narrow(&mut self.vars, &predicate, true);
            }
            (false, false) => {
                self.vars = then_env;
                self.merge_env(&else_env, if_node.subsequent().is_none());
            }
        }

        // Type union — exclude diverging branches.
        join_branch_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    /// Track a case statement with branch merging
    ///
    /// Each when clause is tracked separately, then all branches
    /// (including else) are merged at the join point.
    fn track_case(&mut self, case_node: &CaseNode) -> RubyType {
        // Track the predicate (the value being matched)
        if let Some(predicate) = case_node.predicate() {
            self.track_node(&predicate);
        }

        let env_before = self.vars.clone();

        // (env, type, diverges) per branch.
        let mut branches: Vec<(HashMap<String, RubyType>, RubyType, bool)> = Vec::new();

        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                self.vars = env_before.clone();
                let diverges = when_node
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
                let branch_type = if let Some(statements) = when_node.statements() {
                    self.track_node(&statements.as_node())
                } else {
                    RubyType::nil_class()
                };
                branches.push((self.vars.clone(), branch_type, diverges));
            }
        }

        let has_else = case_node.else_clause().is_some();
        if has_else {
            self.vars = env_before.clone();
            let else_clause = case_node.else_clause().unwrap();
            let diverges = else_clause
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
            let else_type = if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), else_type, diverges));
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        // Pick post-state from non-diverging branches only.
        let surviving_envs: Vec<&HashMap<String, RubyType>> = branches
            .iter()
            .filter(|(_, _, d)| !*d)
            .map(|(env, _, _)| env)
            .collect();

        if surviving_envs.is_empty() {
            // All branches diverge — code after is unreachable. Keep pre-state.
            self.vars = env_before;
        } else {
            self.vars = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
            if !has_else {
                for (var, ty) in self.vars.clone() {
                    let union = RubyType::union(vec![ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        }

        // Type union — exclude diverging branches.
        let typed_branches: Vec<(RubyType, bool)> =
            branches.into_iter().map(|(_, ty, d)| (ty, d)).collect();
        join_branch_types(&typed_branches)
    }

    /// Track a while loop with limited iterations
    ///
    /// Iterates the loop body a few times to allow types to stabilize,
    /// then merges with the pre-loop state (since loop might not execute).
    fn track_while(&mut self, while_node: &WhileNode) -> RubyType {
        // Track the predicate
        let predicate = while_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.vars.clone();

        // Iterate loop body a limited number of times
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..self.max_loop_iterations {
            if let Some(statements) = while_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }

        // Save post-loop state
        let loop_env = self.vars.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.vars = env_before.clone();
        self.merge_env(&loop_env, true); // true = loop might not run

        last_type
    }

    /// Track an until loop (inverse of while)
    fn track_until(&mut self, until_node: &UntilNode) -> RubyType {
        // Track the predicate
        let predicate = until_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.vars.clone();

        // Iterate loop body a limited number of times
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..self.max_loop_iterations {
            if let Some(statements) = until_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }

        // Save post-loop state
        let loop_env = self.vars.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.vars = env_before.clone();
        self.merge_env(&loop_env, true); // true = loop might not run

        last_type
    }

    /// Track an unless statement (inverse of if)
    fn track_unless(&mut self, unless_node: &UnlessNode) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = unless_node.predicate();
        self.track_node(&predicate);

        let env_before = self.vars.clone();

        // Then branch (executes when predicate is false)
        let then_diverges = unless_node
            .statements()
            .map(|s| control_flow::diverges(&s.as_node()))
            .unwrap_or(false);
        let then_type = if let Some(statements) = unless_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.vars.clone();

        self.vars = env_before.clone();

        // Else branch
        let else_diverges = unless_node
            .else_clause()
            .and_then(|e| e.statements())
            .map(|s| control_flow::diverges(&s.as_node()))
            .unwrap_or(false);
        let else_type = if let Some(else_clause) = unless_node.else_clause() {
            if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            }
        } else {
            RubyType::nil_class()
        };
        let else_env = self.vars.clone();

        match (then_diverges, else_diverges) {
            (true, true) => self.vars = env_before,
            (true, false) => {
                // unless body (executes when predicate FALSE) exited → at join, predicate was true.
                self.vars = else_env;
                narrow::narrow(&mut self.vars, &predicate, true);
            }
            (false, true) => {
                // else (executes when predicate TRUE) exited → at join, predicate was false.
                self.vars = then_env;
                narrow::narrow(&mut self.vars, &predicate, false);
            }
            (false, false) => {
                self.vars = then_env;
                self.merge_env(&else_env, unless_node.else_clause().is_none());
            }
        }

        join_branch_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    /// Infer the type of an expression
    ///
    /// Uses literal analyzer for static types, and handles variable reads
    /// by looking up their type from the current environment.
    fn infer_expression(&mut self, node: &Node) -> RubyType {
        // Try literal analysis first
        if let Some(ty) = self.literal_analyzer.analyze_literal(node) {
            return ty;
        }

        // Handle local variable reads
        if let Some(read) = node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(read.name().as_slice()).to_string();
            if let Some(ty) = self.vars.get(&var_name) {
                return ty.clone();
            }
            return RubyType::Unknown;
        }

        // Handle method calls
        if let Some(call) = node.as_call_node() {
            return self.infer_call(&call);
        }

        // Handle return statements
        if let Some(ret) = node.as_return_node() {
            return self.infer_return(&ret);
        }

        // Handle constant reads (class references)
        if let Some(const_read) = node.as_constant_read_node() {
            let const_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
            if let Ok(constant) = RubyConstant::new(&const_name) {
                return RubyType::ClassReference(FullyQualifiedName::Constant(vec![constant]));
            }
        }

        // Handle constant path (namespaced constants like Foo::Bar)
        if let Some(const_path) = node.as_constant_path_node() {
            if let Some(fqn) = self.resolve_constant_path(&const_path) {
                return RubyType::ClassReference(fqn);
            }
        }

        // Handle parenthesized expressions
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.infer_expression(&body);
            }
            return RubyType::nil_class();
        }

        // Handle interpolated strings
        if node.as_interpolated_string_node().is_some() {
            return RubyType::string();
        }

        RubyType::Unknown
    }

    /// Infer the return type of a method call
    fn infer_call(&mut self, call: &CallNode) -> RubyType {
        let method_name = String::from_utf8_lossy(call.name().as_slice()).to_string();

        // Handle .new specially - it returns an instance of the class
        if method_name == "new" {
            if let Some(receiver) = call.receiver() {
                if let Some(const_read) = receiver.as_constant_read_node() {
                    let class_name =
                        String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                    if let Ok(constant) = RubyConstant::new(&class_name) {
                        let fqn = FullyQualifiedName::Constant(vec![constant]);
                        return RubyType::Class(fqn);
                    }
                }
                // Handle namespaced constant like Foo::Bar.new
                if let Some(const_path) = receiver.as_constant_path_node() {
                    if let Some(fqn) = self.resolve_constant_path(&const_path) {
                        return RubyType::Class(fqn);
                    }
                }
            }
        }

        // Get receiver type
        let receiver_type = if let Some(receiver) = call.receiver() {
            self.infer_expression(&receiver)
        } else {
            // Implicit self - use current class context
            if let Some(ref fqn) = self.current_class {
                RubyType::Class(fqn.clone())
            } else {
                RubyType::Unknown
            }
        };

        // If receiver is Unknown, propagate Unknown (no global lookup)
        if receiver_type == RubyType::Unknown {
            return RubyType::Unknown;
        }

        if let Some(return_type) =
            self.resolve_method_return_type_from_analysis(&receiver_type, &method_name)
        {
            return return_type;
        }

        self.resolve_rbs_method_return_type(&receiver_type, &method_name)
            .unwrap_or(RubyType::Unknown)
    }

    fn resolve_rbs_method_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let is_singleton = matches!(
            receiver_type,
            RubyType::ClassReference(_) | RubyType::ModuleReference(_)
        );
        let class_name = match receiver_type {
            RubyType::Class(fqn)
            | RubyType::ClassReference(fqn)
            | RubyType::Module(fqn)
            | RubyType::ModuleReference(fqn) => fqn.namespace_parts().last().map(|c| c.to_string()),
            RubyType::Array(_) => Some("Array".to_string()),
            RubyType::Hash(_, _) => Some("Hash".to_string()),
            RubyType::Union(_) | RubyType::Unknown => None,
        }?;
        crate::inferrer::rbs::get_rbs_method_return_type_as_ruby_type(
            &class_name,
            method_name,
            is_singleton,
        )
    }

    fn resolve_method_return_type_from_analysis(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let analysis_engine = self.analysis_engine.as_ref()?;
        let method = crate::types::ruby_method::RubyMethod::new(method_name).ok()?;
        let (receiver_fqn, namespace_kind) = match receiver_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                (fqn.clone(), ruby_analysis_core::NamespaceKind::Instance)
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                (fqn.clone(), ruby_analysis_core::NamespaceKind::Singleton)
            }
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Union(_) | RubyType::Unknown => {
                return None;
            }
        };
        let namespace =
            FullyQualifiedName::namespace_with_kind(receiver_fqn.namespace_parts(), namespace_kind);
        let engine = analysis_engine.lock();
        let query = AnalysisQuery::new(&engine);
        query.method_return_type_for_receiver(&namespace, &method)
    }

    /// Infer the type of a return statement
    fn infer_return(&mut self, ret: &ReturnNode) -> RubyType {
        if let Some(args) = ret.arguments() {
            let args_list: Vec<_> = args.arguments().iter().collect();
            if args_list.is_empty() {
                return RubyType::nil_class();
            } else if args_list.len() == 1 {
                return self.infer_expression(&args_list[0]);
            } else {
                // Multiple return values become an array
                let types: Vec<RubyType> =
                    args_list.iter().map(|a| self.infer_expression(a)).collect();
                return RubyType::Array(types);
            }
        }
        RubyType::nil_class()
    }

    /// Resolve a constant path to an FQN (e.g., Foo::Bar::Baz)
    fn resolve_constant_path(&self, const_path: &ConstantPathNode) -> Option<FullyQualifiedName> {
        let mut parts = Vec::new();

        // Get the child constant name
        if let Some(name_node) = const_path.name() {
            let name = String::from_utf8_lossy(name_node.as_slice()).to_string();
            parts.push(RubyConstant::new(&name).ok()?);
        }

        // Get parent parts recursively
        if let Some(parent) = const_path.parent() {
            if let Some(parent_path) = parent.as_constant_path_node() {
                if let Some(parent_fqn) = self.resolve_constant_path(&parent_path) {
                    if let FullyQualifiedName::Constant(parent_parts) = parent_fqn {
                        let mut full_parts = parent_parts;
                        full_parts.extend(parts);
                        return Some(FullyQualifiedName::Constant(full_parts));
                    }
                }
            } else if let Some(const_read) = parent.as_constant_read_node() {
                let parent_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                let mut full_parts = vec![RubyConstant::new(&parent_name).ok()?];
                full_parts.extend(parts);
                return Some(FullyQualifiedName::Constant(full_parts));
            }
        } else {
            // No parent means this is a top-level constant
            return Some(FullyQualifiedName::Constant(parts));
        }

        None
    }

    /// Merge another environment into this one
    ///
    /// Used at control flow join points (after if/case/while).
    /// Variables with different types are merged into unions.
    ///
    /// If `no_else_branch` is true, variables that only exist in one branch
    /// are assumed to be nil in the other branch.
    fn merge_env(&mut self, other_env: &HashMap<String, RubyType>, no_else_branch: bool) {
        // For each variable in other environment
        for (var, other_ty) in other_env {
            if let Some(this_ty) = self.vars.get(var) {
                // Variable exists in both - create union if types differ
                if this_ty != other_ty {
                    let union = RubyType::union(vec![this_ty.clone(), other_ty.clone()]);
                    self.vars.insert(var.clone(), union);
                }
            } else {
                // Variable only in other environment
                if no_else_branch {
                    // No else branch: variable might not be defined
                    let union = RubyType::union(vec![other_ty.clone(), RubyType::nil_class()]);
                    self.vars.insert(var.clone(), union);
                } else {
                    // Has else branch: variable was defined in else but not then
                    // Add with nil union
                    let union = RubyType::union(vec![other_ty.clone(), RubyType::nil_class()]);
                    self.vars.insert(var.clone(), union);
                }
            }
        }

        // Handle variables only in this environment (they might be nil in other)
        if no_else_branch {
            // If there's no else branch, variables in then branch might be undefined
            // when the condition is false
            for (var, this_ty) in self.vars.clone() {
                if !other_env.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        } else {
            // Has else branch: variables in then but not else get nil union
            for (var, this_ty) in self.vars.clone() {
                if !other_env.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        }
    }
}

/// Join branch result types into the surrounding expression's type, excluding
/// branches that always diverge (return/raise/break/...). The join point is
/// never reached via a diverging branch, so its result type is irrelevant.
///
/// All branches diverge → `Unknown` (Bottom-equivalent; downstream consumers
/// already treat this as "no information").
fn join_branch_types(branches: &[(RubyType, bool)]) -> RubyType {
    let surviving: Vec<RubyType> = branches
        .iter()
        .filter(|(_, diverges)| !*diverges)
        .map(|(ty, _)| ty.clone())
        .collect();
    if surviving.is_empty() {
        RubyType::Unknown
    } else {
        RubyType::union(surviving)
    }
}

/// Helper to get type at offset from var_types BTreeMap
pub fn get_var_type_at(
    var_types: &BTreeMap<usize, HashMap<String, RubyType>>,
    offset: usize,
    var_name: &str,
) -> Option<RubyType> {
    var_types
        .range(..=offset)
        .next_back()
        .and_then(|(_, vars)| vars.get(var_name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tracker<'a>(source: &'a str, uri: &'a Url) -> TypeTracker<'a> {
        TypeTracker::new(source.as_bytes(), uri)
    }

    #[test]
    fn test_simple_method_tracking() {
        let source = "def foo\n  5\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        let return_type = tracker.track_method(&def_node);

        assert_eq!(return_type, RubyType::integer());
    }

    #[test]
    fn test_local_variable_assignment() {
        let source = "def foo\n  x = 5\n  x\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // Check that var_types were recorded
        assert!(!var_types.is_empty());

        // Find the assignment offset (after "x = 5")
        let assignment_end_offset = source.find("x = 5").unwrap() + "x = 5".len();

        // Query type after assignment
        let x_type = get_var_type_at(&var_types, assignment_end_offset, "x");
        assert_eq!(x_type, Some(RubyType::integer()));
    }

    #[test]
    fn test_multiple_assignments() {
        let source = "def foo\n  x = 5\n  y = \"hello\"\n  x\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // Find offset after both assignments
        let second_assignment_end = source.find("y = \"hello\"").unwrap() + "y = \"hello\"".len();

        // Both variables should be in the environment
        let x_type = get_var_type_at(&var_types, second_assignment_end, "x");
        let y_type = get_var_type_at(&var_types, second_assignment_end, "y");

        assert_eq!(x_type, Some(RubyType::integer()));
        assert_eq!(y_type, Some(RubyType::string()));
    }

    #[test]
    fn test_reassignment_changes_type() {
        let source = "def foo\n  x = 5\n  x = \"hello\"\n  x\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After first assignment, should be Integer
        let first_assignment_end = source.find("x = 5").unwrap() + "x = 5".len();
        let x_type_1 = get_var_type_at(&var_types, first_assignment_end, "x");
        assert_eq!(x_type_1, Some(RubyType::integer()));

        // After second assignment, should be String
        let second_assignment_end = source.find("x = \"hello\"").unwrap() + "x = \"hello\"".len();
        let x_type_2 = get_var_type_at(&var_types, second_assignment_end, "x");
        assert_eq!(x_type_2, Some(RubyType::string()));
    }

    #[test]
    fn test_if_with_else() {
        let source = r#"def foo
  if true
    x = 5
  else
    x = "hello"
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the if statement, x should be Integer | String
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_if, "x");

        // Should be a union type containing both Integer and String
        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_if_without_else() {
        let source = r#"def foo
  if true
    x = 5
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the if statement, x should be Integer | NilClass (might not be defined)
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_if, "x");

        // Should be a union type containing Integer and NilClass
        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_unless_statement() {
        let source = r#"def foo
  unless false
    x = 5
  else
    x = "hello"
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the unless statement, x should be Integer | String
        let after_unless = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_unless, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_elsif_chain() {
        let source = r#"def foo
  if true
    x = 5
  elsif false
    x = "hello"
  else
    x = 3.14
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the if/elsif/else, x should be a union of all three types
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_if, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_case_with_else() {
        let source = r#"def foo
  case value
  when 1
    x = 5
  when 2
    x = "hello"
  else
    x = 3.14
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the case statement, x should be Integer | String | Float
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_case, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_case_without_else() {
        let source = r#"def foo
  case value
  when 1
    x = 5
  when 2
    x = "hello"
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the case statement, x should be Integer | String | NilClass
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_case, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_case_single_branch() {
        let source = r#"def foo
  case value
  when 1
    x = 5
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the case statement, x should be Integer | NilClass
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_case, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_while_loop() {
        let source = r#"def foo
  x = 0
  while true
    x = 5
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the while loop, x should be Integer (0 or 5)
        let after_while = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_while, "x");

        assert!(x_type.is_some());
        // Type should still be Integer (union of Integer | Integer = Integer)
    }

    #[test]
    fn test_until_loop() {
        let source = r#"def foo
  x = 0
  until false
    x = "hello"
  end
  x
end"#;
        let uri = Url::parse("file:///test.rb").unwrap();
        let mut tracker = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the until loop, x should be Integer | String
        let after_until = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_until, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }
}
