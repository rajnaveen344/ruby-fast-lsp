//! # Deterministic Simulation Testing for Ruby Fast LSP
//!
//! This module implements stateful property-based testing using `proptest`.
//! The core idea: define an abstract Model (perfect simplification) and verify
//! that the real LSP implementation never diverges from it.
//!
//! ## Architecture
//!
//! - `model.rs`: The Model (Oracle) - a simple HashMap tracking expected state
//! - `transitions.rs`: All possible LSP operations (DidOpen, DidChange, etc.)
//! - `generators.rs`: Strategies for generating Ruby content and valid edits
//! - `harness.rs`: Test harness connecting Model to real LSP server
//! - `tests.rs`: Property-based tests using proptest
//!
//! ## Assertion Levels
//!
//! - Level 1: No panics, text synchronization (Model.text == LSP.text)
//! - Level 2: Semantic correctness via marker strategy (constructed inputs)
//! - Level 3: Mathematical properties (idempotency, determinism)
//!
//! ## Usage
//!
//! ```bash
//! # Run simulation tests
//! cargo test simulation
//!
//! # Reproduce a specific failure
//! PROPTEST_SEED=0x1234 cargo test simulation
//!
//! # Run with more iterations
//! PROPTEST_CASES=500 cargo test simulation
//! ```

mod generators;
mod harness;
mod model;
mod tests;
mod transitions;

pub use generators::*;
pub use harness::*;
pub use model::*;
pub use transitions::*;

/// Model version for seed compatibility tracking.
/// Bump this when changing transitions, generators, or weights.
pub const MODEL_VERSION: &str = "v1";
