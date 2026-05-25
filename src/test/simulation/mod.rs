//! Deterministic project simulation tests.
//!
//! The simulation model describes a Ruby project graph in Rust, generates Ruby
//! files, drives all edits through `FakeEditor`, then compares LSP/index
//! observations against the model oracle.

mod engine_runner;
mod graph;
mod oracle;
mod project;
mod ruby_gen;
mod runner;
mod seeded;
mod tests;

pub use engine_runner::*;
pub use graph::*;
pub use oracle::*;
pub use project::*;
pub use runner::*;
pub use seeded::*;
