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

use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::method::MethodResolver;
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use ruby_prism::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

/// Type snapshot valid for a specific offset range in the source code.
///
/// Each snapshot represents the type state of all variables within a specific
/// byte range. Snapshots are created after each statement and at control flow
/// merge points.
#[derive(Debug, Clone)]
pub struct TypeSnapshot {
    /// Start of range (inclusive)
    pub start_offset: usize,

    /// End of range (exclusive)
    pub end_offset: usize,

    /// Variable types valid in this range
    pub variables: HashMap<String, RubyType>,
}

impl TypeSnapshot {
    /// Create a new type snapshot for the given range
    pub fn new(start_offset: usize, end_offset: usize) -> Self {
        Self {
            start_offset,
            end_offset,
            variables: HashMap::new(),
        }
    }

    /// Check if this snapshot contains the given offset
    pub fn contains(&self, offset: usize) -> bool {
        self.start_offset <= offset && offset < self.end_offset
    }
}

/// Simple forward type tracker with control flow merging.
///
/// Performs a single forward pass through a method's AST, tracking variable
/// types and creating snapshots at each statement. Handles control flow by
/// cloning the environment for branches and merging at join points.
pub struct TypeTracker<'a> {
    /// Current type environment (variable name → type)
    vars: HashMap<String, RubyType>,

    /// Snapshots at each statement (for queries)
    /// Sorted by start_offset for binary search
    snapshots: Vec<TypeSnapshot>,

    /// Source code (for offset calculations)
    #[allow(dead_code)]
    source: &'a [u8],

    /// Literal analyzer (for static type inference)
    literal_analyzer: LiteralAnalyzer,

    /// Index for method return type lookups
    index: Index<Unlocked>,

    /// Current URI (for cross-file lookups)
    #[allow(dead_code)]
    uri: &'a Url,

    /// Max loop iterations (to prevent infinite loops)
    max_loop_iterations: usize,

    /// Track the last snapshot end offset to determine range starts
    last_snapshot_end: usize,

    /// Current class/module context for resolving implicit self
    current_class: Option<FullyQualifiedName>,
}

impl<'a> TypeTracker<'a> {
    /// Create a new type tracker for the given source and index
    pub fn new(source: &'a [u8], index: Index<Unlocked>, uri: &'a Url) -> Self {
        Self {
            vars: HashMap::new(),
            snapshots: Vec::new(),
            source,
            literal_analyzer: LiteralAnalyzer::new(),
            index,
            uri,
            max_loop_iterations: 10,
            last_snapshot_end: 0,
            current_class: None,
        }
    }

    /// Set the current class/module context for resolving implicit self
    pub fn set_current_class(&mut self, fqn: Option<FullyQualifiedName>) {
        self.current_class = fqn;
    }

    /// Get all snapshots (for storing in RubyDocument)
    pub fn snapshots(&self) -> &[TypeSnapshot] {
        &self.snapshots
    }

    /// Take snapshot for range [start, end)
    ///
    /// Creates a new snapshot capturing the current variable types for the
    /// specified byte offset range.
    fn snapshot(&mut self, start_offset: usize, end_offset: usize) {
        let mut snapshot = TypeSnapshot::new(start_offset, end_offset);
        snapshot.variables = self.vars.clone();
        self.snapshots.push(snapshot);
        self.last_snapshot_end = end_offset;
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

        // Final snapshot for any remaining code
        if let Some(body) = method.body() {
            let end_offset = body.location().end_offset();
            if self.last_snapshot_end < end_offset {
                self.snapshot(self.last_snapshot_end, end_offset);
            }
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
        let stmts_vec: Vec<_> = stmts.body().iter().collect();

        for (i, stmt) in stmts_vec.iter().enumerate() {
            // Process the statement (this updates self.vars)
            last_type = self.track_node(&stmt);
            let stmt_end = stmt.location().end_offset();

            // Determine the range for this snapshot
            // The snapshot represents the type state AFTER this statement
            // The range should be from the end of this statement to the start of the next
            let snapshot_start = stmt_end;
            let snapshot_end = if i + 1 < stmts_vec.len() {
                stmts_vec[i + 1].location().start_offset()
            } else {
                // Last statement: snapshot extends to the end of the statements block
                stmts.location().end_offset()
            };

            // Create snapshot with the post-execution state
            if snapshot_end > snapshot_start {
                self.snapshot(snapshot_start, snapshot_end);
            }
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

        // Save the current environment before branching
        let env_before = self.vars.clone();
        let snapshots_before = self.snapshots.len();

        // Track the then branch
        let then_type = if let Some(statements) = if_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };

        // Save then branch state
        let then_env = self.vars.clone();
        let then_snapshots = self.snapshots.clone();

        // Reset to pre-branch state for else branch
        self.vars = env_before.clone();
        self.snapshots.truncate(snapshots_before);

        // Track the else branch
        let else_type = if let Some(subsequent) = if_node.subsequent() {
            // subsequent could be ElseNode or another IfNode (elsif)
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
                    // Handle elsif as another if
                    let elsif_node = subsequent.as_if_node().unwrap();
                    self.track_if(&elsif_node)
                }
                _ => RubyType::nil_class(),
            }
        } else {
            // No else branch - variables might be undefined
            RubyType::nil_class()
        };

        // Save else branch state
        let else_env = self.vars.clone();
        let else_snapshots = self.snapshots.clone();

        // Merge the two branches
        // Combine snapshots from both branches
        self.snapshots = then_snapshots;
        self.snapshots
            .extend(else_snapshots.iter().skip(snapshots_before).cloned());

        // Merge environments
        self.vars = then_env;
        self.merge_env(&else_env, if_node.subsequent().is_none());

        // Return type is union of both branches
        RubyType::union(vec![then_type, else_type])
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

        // Save the current environment before branching
        let env_before = self.vars.clone();
        let snapshots_before = self.snapshots.len();

        // Track all when branches and collect their environments
        let mut branch_envs = Vec::new();
        let mut branch_types = Vec::new();
        let mut all_snapshots = Vec::new();

        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                // Reset to pre-branch state for this when clause
                self.vars = env_before.clone();
                self.snapshots.truncate(snapshots_before);

                // Track this when branch
                let branch_type = if let Some(statements) = when_node.statements() {
                    self.track_node(&statements.as_node())
                } else {
                    RubyType::nil_class()
                };

                // Save this branch's state
                branch_envs.push(self.vars.clone());
                branch_types.push(branch_type);
                all_snapshots.push(self.snapshots.clone());
            }
        }

        // Track the else clause if present
        let has_else = case_node.else_clause().is_some();
        if has_else {
            // Reset to pre-branch state for else
            self.vars = env_before.clone();
            self.snapshots.truncate(snapshots_before);

            let else_type = if let Some(else_clause) = case_node.else_clause() {
                if let Some(statements) = else_clause.statements() {
                    self.track_node(&statements.as_node())
                } else {
                    RubyType::nil_class()
                }
            } else {
                RubyType::nil_class()
            };

            branch_envs.push(self.vars.clone());
            branch_types.push(else_type);
            all_snapshots.push(self.snapshots.clone());
        }

        // Merge all branches
        if branch_envs.is_empty() {
            // No when clauses - just return nil
            return RubyType::nil_class();
        }

        // Start with the first branch
        self.vars = branch_envs[0].clone();
        self.snapshots = all_snapshots[0].clone();

        // Merge all other branches
        for i in 1..branch_envs.len() {
            self.merge_env(&branch_envs[i], false);
            self.snapshots
                .extend(all_snapshots[i].iter().skip(snapshots_before).cloned());
        }

        // If there's no else clause, variables might be undefined
        if !has_else {
            // Add nil to all variables that were defined in any branch
            for (var, ty) in self.vars.clone() {
                let union = RubyType::union(vec![ty, RubyType::nil_class()]);
                self.vars.insert(var, union);
            }
        }

        // Return type is union of all branches
        RubyType::union(branch_types)
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
        let snapshots_before = self.snapshots.len();

        // Iterate loop body a limited number of times
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..self.max_loop_iterations {
            if let Some(statements) = while_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }

            // Check if types have stabilized (optimization: could compare vars)
            // For simplicity, just run max iterations
        }

        // Save post-loop state
        let loop_env = self.vars.clone();
        let loop_snapshots = self.snapshots.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.vars = env_before.clone();
        self.snapshots.truncate(snapshots_before);
        self.merge_env(&loop_env, true); // true = loop might not run

        // Restore snapshots from loop execution
        self.snapshots = loop_snapshots;

        last_type
    }

    /// Track an until loop (inverse of while)
    fn track_until(&mut self, until_node: &UntilNode) -> RubyType {
        // Track the predicate
        let predicate = until_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.vars.clone();
        let snapshots_before = self.snapshots.len();

        // Iterate loop body a limited number of times
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..self.max_loop_iterations {
            if let Some(statements) = until_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }

        // Save post-loop state
        let loop_env = self.vars.clone();
        let loop_snapshots = self.snapshots.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.vars = env_before.clone();
        self.snapshots.truncate(snapshots_before);
        self.merge_env(&loop_env, true); // true = loop might not run

        // Restore snapshots from loop execution
        self.snapshots = loop_snapshots;

        last_type
    }

    /// Track an unless statement (inverse of if)
    fn track_unless(&mut self, unless_node: &UnlessNode) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = unless_node.predicate();
        self.track_node(&predicate);

        // Save the current environment before branching
        let env_before = self.vars.clone();
        let snapshots_before = self.snapshots.len();

        // Track the then branch (executes when predicate is false)
        let then_type = if let Some(statements) = unless_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };

        // Save then branch state
        let then_env = self.vars.clone();
        let then_snapshots = self.snapshots.clone();

        // Reset to pre-branch state for else branch
        self.vars = env_before.clone();
        self.snapshots.truncate(snapshots_before);

        // Track the else branch
        let else_type = if let Some(else_clause) = unless_node.else_clause() {
            if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            }
        } else {
            RubyType::nil_class()
        };

        // Save else branch state
        let else_env = self.vars.clone();
        let else_snapshots = self.snapshots.clone();

        // Merge the two branches
        self.snapshots = then_snapshots;
        self.snapshots
            .extend(else_snapshots.iter().skip(snapshots_before).cloned());

        // Merge environments
        self.vars = then_env;
        self.merge_env(&else_env, unless_node.else_clause().is_none());

        // Return type is union of both branches
        RubyType::union(vec![then_type, else_type])
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

        // Use MethodResolver to look up return type from index and RBS
        let index = self.index.lock();
        MethodResolver::resolve_method_return_type(&index, &receiver_type, &method_name)
            .unwrap_or(RubyType::Unknown)
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

/// Query snapshots for type at a specific offset
///
/// Uses binary search to efficiently find the snapshot containing the offset.
/// Snapshots are end-inclusive for boundary cases (start <= offset <= end).
pub fn get_type_at_offset(
    snapshots: &[TypeSnapshot],
    offset: usize,
    var_name: &str,
) -> Option<RubyType> {
    // First, try exact match - find snapshot containing offset
    let exact_match = snapshots
        .binary_search_by(|snapshot| {
            if snapshot.end_offset < offset {
                Ordering::Less
            } else if snapshot.start_offset > offset {
                Ordering::Greater
            } else {
                Ordering::Equal // Found: start <= offset <= end
            }
        })
        .ok()
        .and_then(|idx| snapshots[idx].variables.get(var_name).cloned());

    if exact_match.is_some() {
        return exact_match;
    }

    // Fallback: Find the last snapshot where start_offset <= offset
    // This handles cases where we're querying beyond the last snapshot's end_offset
    // (e.g., using a variable later in the code after it was assigned)
    snapshots
        .iter()
        .rev()
        .find(|s| s.start_offset <= offset)
        .and_then(|s| s.variables.get(var_name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use parking_lot::Mutex;
    use std::sync::Arc;

    fn create_test_tracker<'a>(
        source: &'a str,
        uri: &'a Url,
    ) -> (TypeTracker<'a>, Index<Unlocked>) {
        let index = Index::new(Arc::new(Mutex::new(RubyIndex::new())));
        let tracker = TypeTracker::new(source.as_bytes(), index.clone(), uri);
        (tracker, index)
    }

    #[test]
    fn test_snapshot_contains_offset() {
        let snapshot = TypeSnapshot::new(10, 20);
        assert!(snapshot.contains(10)); // Start inclusive
        assert!(snapshot.contains(15)); // Middle
        assert!(!snapshot.contains(20)); // End exclusive
        assert!(!snapshot.contains(5)); // Before
        assert!(!snapshot.contains(25)); // After
    }

    #[test]
    fn test_get_type_at_offset() {
        let snapshots = vec![
            TypeSnapshot {
                start_offset: 0,
                end_offset: 10,
                variables: HashMap::new(),
            },
            TypeSnapshot {
                start_offset: 10,
                end_offset: 20,
                variables: vec![("x".to_string(), RubyType::integer())]
                    .into_iter()
                    .collect(),
            },
            TypeSnapshot {
                start_offset: 20,
                end_offset: 30,
                variables: vec![("x".to_string(), RubyType::string())]
                    .into_iter()
                    .collect(),
            },
        ];

        // Query before any assignments
        assert_eq!(get_type_at_offset(&snapshots, 5, "x"), None);

        // Query in first range with x: Integer
        assert_eq!(
            get_type_at_offset(&snapshots, 15, "x"),
            Some(RubyType::integer())
        );

        // Query in second range with x: String
        assert_eq!(
            get_type_at_offset(&snapshots, 25, "x"),
            Some(RubyType::string())
        );

        // Query at exact boundaries
        assert_eq!(
            get_type_at_offset(&snapshots, 10, "x"),
            Some(RubyType::integer())
        );
        assert_eq!(
            get_type_at_offset(&snapshots, 20, "x"),
            Some(RubyType::string())
        );
    }

    #[test]
    fn test_simple_method_tracking() {
        let source = "def foo\n  5\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let (mut tracker, _index) = create_test_tracker(source, &uri);

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        // Check that snapshots were created
        let snapshots = tracker.snapshots();
        assert!(snapshots.len() > 0);

        // Find the assignment offset (after "x = 5")
        let assignment_end_offset = source.find("x = 5").unwrap() + "x = 5".len();

        // Query type after assignment
        let x_type = get_type_at_offset(snapshots, assignment_end_offset, "x");
        assert_eq!(x_type, Some(RubyType::integer()));
    }

    #[test]
    fn test_multiple_assignments() {
        let source = "def foo\n  x = 5\n  y = \"hello\"\n  x\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // Find offset after both assignments
        let second_assignment_end = source.find("y = \"hello\"").unwrap() + "y = \"hello\"".len();

        // Both variables should be in the environment
        let x_type = get_type_at_offset(snapshots, second_assignment_end, "x");
        let y_type = get_type_at_offset(snapshots, second_assignment_end, "y");

        assert_eq!(x_type, Some(RubyType::integer()));
        assert_eq!(y_type, Some(RubyType::string()));
    }

    #[test]
    fn test_reassignment_changes_type() {
        let source = "def foo\n  x = 5\n  x = \"hello\"\n  x\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After first assignment, should be Integer
        let first_assignment_end = source.find("x = 5").unwrap() + "x = 5".len();
        let x_type_1 = get_type_at_offset(snapshots, first_assignment_end, "x");
        assert_eq!(x_type_1, Some(RubyType::integer()));

        // After second assignment, should be String
        let second_assignment_end = source.find("x = \"hello\"").unwrap() + "x = \"hello\"".len();
        let x_type_2 = get_type_at_offset(snapshots, second_assignment_end, "x");
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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the if statement, x should be Integer | String
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_if, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the if statement, x should be Integer | NilClass (might not be defined)
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_if, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the unless statement, x should be Integer | String
        let after_unless = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_unless, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the if/elsif/else, x should be a union of all three types
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_if, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the case statement, x should be Integer | String | Float
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_case, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the case statement, x should be Integer | String | NilClass
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_case, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the case statement, x should be Integer | NilClass
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_case, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the while loop, x should be Integer (0 or 5)
        let after_while = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_while, "x");

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
        let (mut tracker, _index) = create_test_tracker(source, &uri);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);

        let snapshots = tracker.snapshots();

        // After the until loop, x should be Integer | String
        let after_until = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_type_at_offset(snapshots, after_until, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }
}
