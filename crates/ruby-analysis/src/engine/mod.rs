//! Editor-agnostic Ruby analysis engine.
//!
//! This crate owns analysis state that should be shared by editor adapters and
//! agent-facing tools. It intentionally has no LSP, parser, or indexer
//! dependency; those layers feed facts into this engine and query deterministic
//! results back out.

mod debug;
mod diagnostic_helpers;
mod diagnostics;
mod engine;
mod file_id_map;
mod hierarchy;
mod lookup;
mod namespace_tree;
mod query;
mod query_types;
mod resolution;
mod types;
mod workspace_symbols;

pub use engine::{AnalysisEngine, FileAnalysisFacts, SourceFile};
pub use file_id_map::FileIdMap;
pub use query::AnalysisQuery;
pub use query_types::{
    AncestorEntry, AncestorsResponse, CallHierarchyMethod, ConstantLookupRequest, ConstantMatch,
    ExportGraphResponse, FileMethodCount, GraphNodeSnapshot, IncluderInfo, IncomingCall,
    InferenceStatsResponse, LocationInfo, LookupEntry, LookupResponse, MethodEntry, MethodMatch,
    MethodsResponse, MixinInfo, MixinUsage, MixinUsageKind, NamespaceNode, NamespaceTreeResponse,
    OutgoingCall, StatsResponse, TypeHierarchyEntry, TypeHierarchyRelation, VariableTypeKind,
    ViaModuleInfo, WorkspaceSymbolMatch,
};
