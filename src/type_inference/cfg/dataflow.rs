//! Dataflow analysis for type narrowing.
//!
//! This module implements forward dataflow analysis to propagate
//! type information through the CFG, applying type guards at
//! branch points.

use std::collections::HashMap;

use crate::type_inference::ruby_type::RubyType;

use super::graph::{BlockId, ControlFlowGraph, EdgeKind, StatementKind};
use super::guards::TypeGuard;

/// Type state at a specific point in the CFG
#[derive(Debug, Clone, Default)]
pub struct TypeState {
    /// Variable name -> type at this point
    pub variables: HashMap<String, RubyType>,
}

impl TypeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from parameter types
    pub fn from_parameters(params: &[(String, RubyType)]) -> Self {
        let mut state = Self::new();
        for (name, ty) in params {
            state.variables.insert(name.clone(), ty.clone());
        }
        state
    }

    /// Get the type of a variable
    pub fn get_type(&self, var_name: &str) -> Option<&RubyType> {
        self.variables.get(var_name)
    }

    /// Set the type of a variable
    pub fn set_type(&mut self, var_name: impl Into<String>, ty: RubyType) {
        self.variables.insert(var_name.into(), ty);
    }

    /// Merge two type states (for join points in CFG)
    pub fn merge(&self, other: &TypeState) -> TypeState {
        let mut result = TypeState::new();

        // Collect all variable names
        let all_vars: std::collections::HashSet<_> = self
            .variables
            .keys()
            .chain(other.variables.keys())
            .collect();

        for var in all_vars {
            let type1 = self.variables.get(var);
            let type2 = other.variables.get(var);

            let merged_type = match (type1, type2) {
                (Some(t1), Some(t2)) => t1.union_with(t2),
                (Some(t), None) | (None, Some(t)) => {
                    // Variable only defined in one branch
                    // Could be undefined in other - union with nil to be safe
                    t.union_with(&RubyType::nil_class())
                }
                (None, None) => continue,
            };

            result.variables.insert(var.clone(), merged_type);
        }

        result
    }

    /// Apply a type guard to narrow types
    pub fn apply_guard(&mut self, guard: &TypeGuard) {
        match guard {
            TypeGuard::IsA {
                variable,
                target_type,
            } => {
                // Narrow to the target type
                self.variables.insert(variable.clone(), target_type.clone());
            }
            TypeGuard::IsNil { variable } => {
                // In this branch, variable is nil
                self.variables
                    .insert(variable.clone(), RubyType::nil_class());
            }
            TypeGuard::NotNil { variable } => {
                // Remove nil from the type
                if let Some(current_type) = self.variables.get(variable) {
                    let narrowed = current_type.clone().remove_nil();
                    self.variables.insert(variable.clone(), narrowed);
                }
            }
            TypeGuard::CaseMatch {
                variable,
                pattern_type,
            } => {
                self.variables
                    .insert(variable.clone(), pattern_type.clone());
            }
            TypeGuard::Equality {
                variable,
                value_type,
            } => {
                self.variables.insert(variable.clone(), value_type.clone());
            }
            TypeGuard::RespondsTo { variable, method } => {
                // Could narrow to types that respond to this method
                // For now, keep the same type (future enhancement)
                let _ = (variable, method);
            }
            TypeGuard::Not(inner) => {
                self.apply_inverse_guard(inner);
            }
            TypeGuard::And(guards) => {
                for g in guards {
                    self.apply_guard(g);
                }
            }
            TypeGuard::Or(guards) => {
                // For OR, we can only narrow if ALL branches narrow the same variable
                // to the same type. This is conservative but safe.
                // For now, don't narrow on OR.
                let _ = guards;
            }
            TypeGuard::Unknown => {
                // No narrowing
            }
        }
    }

    /// Apply the inverse of a type guard
    fn apply_inverse_guard(&mut self, guard: &TypeGuard) {
        match guard {
            TypeGuard::IsA {
                variable,
                target_type,
            } => {
                // Not is_a?(T) means remove T from the union
                if let Some(current_type) = self.variables.get(variable) {
                    let narrowed = current_type.subtract(target_type);
                    self.variables.insert(variable.clone(), narrowed);
                }
            }
            TypeGuard::IsNil { variable } => {
                // Not nil means remove nil
                if let Some(current_type) = self.variables.get(variable) {
                    let narrowed = current_type.clone().remove_nil();
                    self.variables.insert(variable.clone(), narrowed);
                }
            }
            TypeGuard::NotNil { variable } => {
                // Not (not nil) means it IS nil
                self.variables
                    .insert(variable.clone(), RubyType::nil_class());
            }
            TypeGuard::Not(inner) => {
                // Double negation
                self.apply_guard(inner);
            }
            TypeGuard::And(guards) => {
                // !(A && B) = !A || !B - can't narrow
                let _ = guards;
            }
            TypeGuard::Or(guards) => {
                // !(A || B) = !A && !B
                for g in guards {
                    self.apply_inverse_guard(g);
                }
            }
            TypeGuard::CaseMatch { .. }
            | TypeGuard::Equality { .. }
            | TypeGuard::RespondsTo { .. }
            | TypeGuard::Unknown => {
                // No inverse narrowing
            }
        }
    }

    /// Check if two states are equal
    pub fn equals(&self, other: &TypeState) -> bool {
        if self.variables.len() != other.variables.len() {
            return false;
        }
        for (var, ty) in &self.variables {
            match other.variables.get(var) {
                Some(other_ty) if ty == other_ty => continue,
                _ => return false,
            }
        }
        true
    }
}

/// Results of dataflow analysis
#[derive(Debug)]
pub struct DataflowResults {
    /// Type state at entry of each block
    pub block_entry_states: HashMap<BlockId, TypeState>,
    /// Type state at exit of each block
    pub block_exit_states: HashMap<BlockId, TypeState>,
}

impl DataflowResults {
    /// Get the type of a variable at a specific position
    pub fn get_type_at(&self, var_name: &str, block_id: BlockId) -> Option<RubyType> {
        self.block_entry_states
            .get(&block_id)
            .and_then(|state| state.get_type(var_name).cloned())
    }

    /// Get the entry state for a block
    pub fn get_entry_state(&self, block_id: BlockId) -> Option<&TypeState> {
        self.block_entry_states.get(&block_id)
    }

    /// Get the exit state for a block
    pub fn get_exit_state(&self, block_id: BlockId) -> Option<&TypeState> {
        self.block_exit_states.get(&block_id)
    }
}

/// Dataflow analyzer that propagates types through the CFG
pub struct DataflowAnalyzer<'a> {
    cfg: &'a ControlFlowGraph,
    /// Type state at entry of each block
    block_entry_states: HashMap<BlockId, TypeState>,
    /// Type state at exit of each block
    block_exit_states: HashMap<BlockId, TypeState>,
}

impl<'a> DataflowAnalyzer<'a> {
    pub fn new(cfg: &'a ControlFlowGraph) -> Self {
        Self {
            cfg,
            block_entry_states: HashMap::new(),
            block_exit_states: HashMap::new(),
        }
    }

    /// Run the dataflow analysis
    pub fn analyze(&mut self, initial_state: TypeState) {
        // Initialize entry block with initial state
        self.block_entry_states
            .insert(self.cfg.entry, initial_state);

        // Iterate until fixed point
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 100;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            // Process blocks in reverse post-order for faster convergence
            for block_id in self.cfg.reverse_post_order() {
                if self.process_block(block_id) {
                    changed = true;
                }
            }
        }

        if iterations >= MAX_ITERATIONS {
            log::warn!(
                "Dataflow analysis did not converge after {} iterations",
                MAX_ITERATIONS
            );
        }

        log::debug!("Dataflow analysis converged in {} iterations", iterations);
    }

    /// Convert to results (consumes the analyzer)
    pub fn into_results(self) -> DataflowResults {
        DataflowResults {
            block_entry_states: self.block_entry_states,
            block_exit_states: self.block_exit_states,
        }
    }

    /// Process a single block, returns true if state changed
    fn process_block(&mut self, block_id: BlockId) -> bool {
        // Compute entry state by merging all predecessor exit states
        let entry_state = self.compute_entry_state(block_id);

        // Check if entry state changed
        let entry_changed = match self.block_entry_states.get(&block_id) {
            Some(old_state) => !old_state.equals(&entry_state),
            None => true,
        };

        if !entry_changed {
            return false;
        }

        self.block_entry_states
            .insert(block_id, entry_state.clone());

        // Compute exit state by processing block statements
        let exit_state = self.compute_exit_state(block_id, entry_state);
        self.block_exit_states.insert(block_id, exit_state);

        true
    }

    /// Compute entry state by merging predecessor states with edge guards
    fn compute_entry_state(&self, block_id: BlockId) -> TypeState {
        let predecessors = self.cfg.get_predecessors(block_id);

        if predecessors.is_empty() {
            // Entry block or unreachable - use stored entry state or empty
            return self
                .block_entry_states
                .get(&block_id)
                .cloned()
                .unwrap_or_default();
        }

        let mut merged_state: Option<TypeState> = None;

        for &pred_id in predecessors {
            if let Some(pred_exit) = self.block_exit_states.get(&pred_id) {
                // Find the edge from pred to this block
                let edge = self
                    .cfg
                    .get_successors(pred_id)
                    .iter()
                    .find(|e| e.to == block_id);

                // Apply edge guard to predecessor's exit state
                let mut state = pred_exit.clone();
                if let Some(edge) = edge {
                    match &edge.kind {
                        EdgeKind::ConditionalTrue(guard) => {
                            state.apply_guard(guard);
                        }
                        EdgeKind::ConditionalFalse(guard) => {
                            state.apply_guard(&guard.negate());
                        }
                        EdgeKind::Unconditional | EdgeKind::Exception | EdgeKind::Return => {
                            // No guard to apply
                        }
                    }
                }

                // Also apply any entry guards on the block itself
                if let Some(block) = self.cfg.get_block(block_id) {
                    for guard in &block.entry_guards {
                        state.apply_guard(guard);
                    }
                }

                // Merge with accumulated state
                merged_state = Some(match merged_state {
                    Some(existing) => existing.merge(&state),
                    None => state,
                });
            }
        }

        merged_state.unwrap_or_default()
    }

    /// Compute exit state by processing statements in the block
    fn compute_exit_state(&self, block_id: BlockId, mut state: TypeState) -> TypeState {
        if let Some(block) = self.cfg.get_block(block_id) {
            for stmt in &block.statements {
                match &stmt.kind {
                    StatementKind::Assignment { target, value_type } => {
                        if let Some(ty) = value_type {
                            state.set_type(target.clone(), ty.clone());
                        }
                    }
                    StatementKind::MethodCall { .. } => {
                        // Could track side effects here
                    }
                    StatementKind::Return { .. } => {
                        // Return doesn't change variable types
                    }
                    StatementKind::Expression => {
                        // Generic expressions don't change types
                    }
                }
            }
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_inference::cfg::CfgBuilder;

    fn analyze_method(source: &str) -> DataflowResults {
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
        let cfg = builder.build_from_method(&method);

        // Create initial state from parameters
        let initial_state = TypeState::from_parameters(&cfg.parameters);

        let mut analyzer = DataflowAnalyzer::new(&cfg);
        analyzer.analyze(initial_state);
        analyzer.into_results()
    }

    #[test]
    fn test_type_state_merge() {
        let mut state1 = TypeState::new();
        state1.set_type("x", RubyType::string());
        state1.set_type("y", RubyType::integer());

        let mut state2 = TypeState::new();
        state2.set_type("x", RubyType::integer());
        state2.set_type("z", RubyType::float());

        let merged = state1.merge(&state2);

        // x should be String | Integer
        let x_type = merged.get_type("x").unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));

        // y should be Integer | nil (only in state1)
        let y_type = merged.get_type("y").unwrap();
        assert!(matches!(y_type, RubyType::Union(_)));

        // z should be Float | nil (only in state2)
        let z_type = merged.get_type("z").unwrap();
        assert!(matches!(z_type, RubyType::Union(_)));
    }

    #[test]
    fn test_type_guard_application() {
        let mut state = TypeState::new();
        state.set_type(
            "x",
            RubyType::Union(vec![
                RubyType::string(),
                RubyType::integer(),
                RubyType::nil_class(),
            ]),
        );

        // Apply is_a?(String) guard
        state.apply_guard(&TypeGuard::is_a("x", RubyType::string()));

        let x_type = state.get_type("x").unwrap();
        assert_eq!(*x_type, RubyType::string());
    }

    #[test]
    fn test_nil_guard_application() {
        let mut state = TypeState::new();
        state.set_type(
            "x",
            RubyType::Union(vec![RubyType::string(), RubyType::nil_class()]),
        );

        // Apply nil? guard (true branch)
        let mut true_state = state.clone();
        true_state.apply_guard(&TypeGuard::is_nil("x"));
        assert_eq!(*true_state.get_type("x").unwrap(), RubyType::nil_class());

        // Apply not nil guard (false branch)
        let mut false_state = state.clone();
        false_state.apply_guard(&TypeGuard::not_nil("x"));
        assert_eq!(*false_state.get_type("x").unwrap(), RubyType::string());
    }

    #[test]
    fn test_simple_dataflow() {
        let source = r#"
def foo
  x = "hello"
  y = 42
end
"#;
        let results = analyze_method(source);

        // Should have at least entry block
        assert!(!results.block_entry_states.is_empty());
    }

    #[test]
    fn test_if_narrowing() {
        let source = r#"
def foo(x)
  if x.nil?
    "was nil"
  else
    x.upcase
  end
end
"#;
        let results = analyze_method(source);

        // Should have multiple blocks
        assert!(results.block_entry_states.len() >= 3);
    }

    #[test]
    fn test_inverse_guard() {
        let mut state = TypeState::new();
        state.set_type(
            "x",
            RubyType::Union(vec![RubyType::string(), RubyType::integer()]),
        );

        // Apply NOT is_a?(String) guard
        let not_string = TypeGuard::Not(Box::new(TypeGuard::is_a("x", RubyType::string())));
        state.apply_guard(&not_string);

        // x should now be just Integer
        let x_type = state.get_type("x").unwrap();
        assert_eq!(*x_type, RubyType::integer());
    }
}
