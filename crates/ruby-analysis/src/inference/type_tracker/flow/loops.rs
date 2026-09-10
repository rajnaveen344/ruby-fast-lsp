use crate::core::RubyType;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

impl TypeTracker {
    /// Track a while loop with limited iterations
    ///
    /// Iterates the loop body a few times to allow types to stabilize,
    /// then merges with the pre-loop state (since loop might not execute).
    pub(in crate::inference::type_tracker) fn track_while(
        &mut self,
        while_node: &WhileNode,
    ) -> RubyType {
        // Track the predicate
        let predicate = while_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.environment.clone();

        // Iterate only the outer loop to avoid exponential work for generated
        // code containing deeply nested loops.
        let iterations = if self.control_flow.loop_depth == 0 {
            self.control_flow.max_loop_iterations
        } else {
            1
        };
        self.control_flow.loop_depth += 1;
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..iterations {
            if let Some(statements) = while_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }
        self.control_flow.loop_depth -= 1;

        // Save post-loop state
        let loop_env = self.environment.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.environment = env_before.clone();
        self.merge_env(&loop_env, true); // true = loop might not run

        last_type
    }

    /// Track an until loop (inverse of while)
    pub(in crate::inference::type_tracker) fn track_until(
        &mut self,
        until_node: &UntilNode,
    ) -> RubyType {
        // Track the predicate
        let predicate = until_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.environment.clone();

        let iterations = if self.control_flow.loop_depth == 0 {
            self.control_flow.max_loop_iterations
        } else {
            1
        };
        self.control_flow.loop_depth += 1;
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..iterations {
            if let Some(statements) = until_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }
        self.control_flow.loop_depth -= 1;

        // Save post-loop state
        let loop_env = self.environment.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.environment = env_before.clone();
        self.merge_env(&loop_env, true); // true = loop might not run

        last_type
    }
}
