//! Type inference tests.
//!
//! Tests for method return type inference, type unioning, branch narrowing,
//! mixin cross-module inference, and related features.

mod attrs;
mod branch_variable_return;
mod call_contexts;
mod inheritance_graphs;
mod method_chaining;
mod mixin_ambiguity;
mod mixin_cross_module;
mod namespace_kind;
mod refinements;
mod return_type_checks;
mod same_file;
mod type_mismatch;
mod type_unioning;
mod unknown_type_propagation;
