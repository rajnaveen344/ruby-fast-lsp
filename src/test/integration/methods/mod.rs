//! Integration tests for methods.
//!
//! Note: hover and inlay_hints tests have been moved to:
//! - src/test/integration/hover/
//! - src/test/integration/inlay_hints/

mod attrs;
mod branch_variable_return;
mod goto;
mod inference;
mod method_chaining;
pub mod mixin_ambiguity;
pub mod mixin_goto;
mod references;
mod refinements;
mod return_type_checks;
mod type_mismatch;
mod type_unioning;
mod unknown_type_propagation;
