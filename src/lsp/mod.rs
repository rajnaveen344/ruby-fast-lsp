//! Editor-facing projections of engine outcomes: LSP request and notification
//! routing, per-feature capability adapters, query adapters over the analysis
//! engine, the `check` CLI report, and the external linter bridge.

pub mod capabilities;
pub mod check;
pub mod handlers;
pub mod linter;
pub mod query;
