//! Inlay hints for variable type annotations.
//!
//! Tests the `: Type` hints shown after variable names.

// Types from the assigned value: literals, constants, constructors, and calls.
pub mod assigned_values;
// Flow-sensitive types: reassignment, branches, rescue, divergence, and guards.
pub mod local_flow;
pub mod ownership;
pub mod unknown;
