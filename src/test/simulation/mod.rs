//! Deterministic project simulation tests.
//!
//! The simulation model describes a Ruby project graph in Rust, generates Ruby
//! files, drives all edits through `FakeEditor`, then compares LSP/index
//! observations against the model oracle.

mod consistency;
mod dependency_refresh;
mod interleavings;
mod oracle_controls;
mod production_schedules;
mod runner;
mod tests;

mod contracts;
pub(crate) use crate::simulation::*;
pub(crate) use crate::simulation::{graph, oracle, project, ruby_gen, seeded};
pub use runner::*;
