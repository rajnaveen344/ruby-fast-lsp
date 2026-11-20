//! Type inference tests.
//!
//! Tests for method return type inference, type unioning, branch narrowing,
//! mixin cross-module inference, and related features.

// Local reads after a block that reassigns the local.
mod block_local_writes;
// Higher-order calls and parameter-dependent callable bodies.
mod callables;
// Local types after `||=`, `&&=`, and operator assignments.
mod compound_local_writes;
// Declared YARD/RBS types checked against inferred types.
mod declared_types;
// Receiver-aware method lookup across contexts, chains, and ancestors.
mod method_resolution;
// Inferred method and accessor return types.
mod return_types;
