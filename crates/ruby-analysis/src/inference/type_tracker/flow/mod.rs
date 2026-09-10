//! Branch environments, bounded loop stabilization, and lexical rescue state.

use crate::inference::type_tracker::flow::rescue::RescueEntryTypes;

pub(in crate::inference::type_tracker) mod branches;
pub(in crate::inference::type_tracker) mod environment;
pub(in crate::inference::type_tracker) mod loops;
pub(in crate::inference::type_tracker) mod narrow;
pub(in crate::inference::type_tracker) mod patterns;
pub(in crate::inference::type_tracker) mod rescue;
pub(in crate::inference::type_tracker) mod shapes;

pub(in crate::inference::type_tracker) struct ControlFlowState {
    /// Max loop iterations (to prevent infinite loops)
    pub(in crate::inference::type_tracker) max_loop_iterations: usize,
    /// Current lexical loop nesting depth. Only the outer loop performs
    /// stabilization iterations; nested loops receive one semantic pass.
    pub(in crate::inference::type_tracker) loop_depth: usize,
    /// Possible local values at every active protected body's rescue entry.
    ///
    /// Each ordinary local assignment records its value both before evaluating
    /// the RHS and after a successful write. An exception can therefore enter
    /// rescue on either side of that write. Nested protected bodies retain one
    /// accumulator each; a write is visible to every enclosing rescue frame
    /// because an inner exception may propagate outward.
    pub(in crate::inference::type_tracker) rescue_entries: Vec<RescueEntryTypes>,
}

impl Default for ControlFlowState {
    fn default() -> Self {
        Self {
            max_loop_iterations: 10,
            loop_depth: 0,
            rescue_entries: Vec::new(),
        }
    }
}
