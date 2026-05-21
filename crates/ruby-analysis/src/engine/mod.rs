//! Editor-agnostic Ruby analysis engine.
//!
//! This crate owns analysis state that should be shared by editor adapters and
//! agent-facing tools. It intentionally has no LSP, parser, or indexer
//! dependency; those layers feed facts into this engine and query deterministic
//! results back out.

mod debug;
mod debug_types;
mod diagnostic_helpers;
mod diagnostics;
mod engine;
mod file_id_map;
mod hierarchy;
mod hierarchy_types;
mod lookup;
mod lookup_types;
mod namespace_tree;
mod namespace_tree_types;
mod query;
mod resolution;
mod types;
mod workspace_symbol_types;
mod workspace_symbols;

pub use debug_types::{
    AncestorEntry, AncestorsResponse, ExportGraphResponse, FileMethodCount, GraphNodeSnapshot,
    InferenceStatsResponse, LookupEntry, LookupResponse, MethodEntry, MethodsResponse,
    StatsResponse,
};
pub use engine::{AnalysisEngine, FileAnalysisFacts, SourceFile};
pub use file_id_map::FileIdMap;
pub use hierarchy_types::{
    CallHierarchyMethod, IncomingCall, OutgoingCall, TypeHierarchyEntry, TypeHierarchyRelation,
};
pub use lookup_types::{
    ConstantLookupRequest, ConstantMatch, MethodMatch, MixinUsage, MixinUsageKind, VariableTypeKind,
};
pub use namespace_tree_types::{
    IncluderInfo, LocationInfo, MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};
pub use query::AnalysisQuery;
pub use workspace_symbol_types::WorkspaceSymbolMatch;
