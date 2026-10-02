//! Editor-facing projections of engine outcomes: LSP request and notification
//! routing, per-feature capability adapters, query adapters over the analysis
//! engine, the `check` CLI report, the external linter bridge, and the `tower-lsp`
//! service facade.

pub mod capabilities;
pub mod check;
pub mod handlers;
pub mod service;
