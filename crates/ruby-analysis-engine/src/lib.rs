//! Editor-agnostic Ruby analysis engine.
//!
//! This crate owns analysis state that should be shared by editor adapters and
//! agent-facing tools. It intentionally has no LSP, parser, or indexer
//! dependency; those layers feed facts into this engine and query deterministic
//! results back out.

mod engine;
mod file_id_map;
mod query;

pub use engine::{AnalysisEngine, FileAnalysisFacts, SourceFile};
pub use file_id_map::FileIdMap;
pub use query::{
    AnalysisQuery, CallHierarchyMethod, IncomingCall, OutgoingCall, WorkspaceSymbolMatch,
};
