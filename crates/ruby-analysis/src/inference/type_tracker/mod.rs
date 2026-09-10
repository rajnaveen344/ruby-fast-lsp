//! Forward Ruby type inference with bounded control-flow and return solving.
//!
//! `traversal` follows Prism nodes. Flow state, semantic lookup inputs, return
//! equations, and recorded type evidence have separate owners. Mutable Hash
//! identities remain local to one pass and are never engine state.

mod context;
mod expressions;
mod flow;
mod observations;
mod returns;
mod traversal;

#[cfg(test)]
mod tests;

use context::{AnalysisContext, MethodContext};
use flow::{environment::FlowEnvironment, ControlFlowState};
use observations::TypeObservations;
use returns::ReturnEvidence;

pub use observations::get_var_type_at;
pub(crate) use observations::LocalReadType;

/// Track a method, program, or isolated block using its existing Prism tree.
#[derive(Default)]
pub struct TypeTracker {
    environment: FlowEnvironment,
    context: MethodContext,
    analysis: AnalysisContext,
    returns: ReturnEvidence,
    observations: TypeObservations,
    control_flow: ControlFlowState,
    /// Keep allocation outside cloned branch environments so identities from
    /// different branches cannot collide at a join.
    next_shape_identity: u32,
}

impl TypeTracker {
    pub fn new() -> Self {
        Self::default()
    }
}
