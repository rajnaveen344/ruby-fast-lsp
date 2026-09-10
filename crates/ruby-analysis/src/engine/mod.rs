//! Editor-agnostic Ruby analysis engine.
//!
//! [`AnalysisEngine`] owns one project's semantic state. Callers register source,
//! replace [`FileFacts`], and read domain results through [`AnalysisQuery`].
//! [`TypeQuery`] provides a file-scoped view of existing type facts.
//!
//! Resolution coordinates the constant and method-return equation solvers in
//! [`crate::inference`], then stores their outcomes through the same file-owned
//! lifecycle. The engine owns lookup policy and solved state; inference owns
//! type rules. Parsing, scheduling, and editor protocol conversion stay outside
//! this module. Stores and their compact representations are internal.

mod debug;
mod debug_types;
mod diagnostic_helpers;
mod diagnostics;
mod external_facts_template;
mod file_id_map;
mod hierarchy;
mod hierarchy_types;
mod lookup;
mod lookup_types;
mod namespace_tree;
mod namespace_tree_types;
mod query;
mod resolution;
mod state;
mod type_query;
mod types;
mod workspace_symbol_types;
mod workspace_symbols;

pub use debug::reference_storage_sizes;
pub use debug_types::{
    AncestorEntry, AncestorsResponse, ExportGraphResponse, FileMethodCount, GraphNodeSnapshot,
    InferenceStatsResponse, LookupEntry, LookupResponse, MethodEntry, MethodsResponse,
    StatsResponse,
};
pub use external_facts_template::{
    ProjectNeutralFileFactsSnapshot, ProjectNeutralFileFactsTemplate,
    ProjectNeutralTemplateRejection,
};
pub use file_id_map::FileIdMap;
pub use hierarchy_types::{
    CallHierarchyMethod, IncomingCall, OutgoingCall, TypeHierarchyEntry, TypeHierarchyNode,
    TypeHierarchyRelation,
};
pub use lookup_types::{
    ConstantHover, ConstantHoverKind, ConstantLookupRequest, ConstantMatch, MethodMatch,
    MixinUsage, MixinUsageKind, VariableTypeKind,
};
pub use namespace_tree_types::{
    IncluderInfo, LibraryNamespaceTree, LibraryPackageTree, LibrarySectionId, LocationInfo,
    MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};
pub use query::AnalysisQuery;
pub use resolution::{ConstantRenameTarget, MethodLookupResult};
pub use state::{
    AnalysisEngine, AnalysisStats, FileFacts, ResolveMode, ResolvePassStats, SemanticChange,
    SemanticExportFingerprint, SemanticResultFingerprint, SourceFile, SourceFileInput,
    SourceFileSnapshot,
};
pub use type_query::TypeQuery;
pub use types::AnalysisQueryCache;
pub use workspace_symbol_types::WorkspaceSymbolMatch;
