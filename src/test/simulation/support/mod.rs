//! Shared simulation model and executable acceptance campaigns.
//! Enabled only for unit tests or the opt-in `simulation` binary.
pub mod build_identity;
pub mod campaigns;
pub mod engine_runner;
pub mod graph;
pub mod oracle;
pub mod project;
pub mod ruby_gen;
pub mod seeded;

pub use engine_runner::*;
pub use graph::*;
pub use oracle::*;
pub use project::*;
pub use seeded::*;
