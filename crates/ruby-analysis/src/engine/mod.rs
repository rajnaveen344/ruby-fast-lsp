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
mod diagnostics;
mod queries;
mod resolution;
mod state;

pub use debug::reference_storage_sizes;
pub use debug::types::{
    AncestorEntry, AncestorsResponse, ExportGraphResponse, FileMethodCount, GraphNodeSnapshot,
    InferenceStatsResponse, LookupEntry, LookupResponse, MethodEntry, MethodsResponse,
    StatsResponse,
};
pub use queries::cache::AnalysisQueryCache;
pub use queries::hierarchy::types::{
    CallHierarchyMethod, IncomingCall, OutgoingCall, TypeHierarchyEntry, TypeHierarchyNode,
    TypeHierarchyRelation,
};
pub use queries::lookup::types::{
    ConstantHover, ConstantHoverKind, ConstantLookupRequest, ConstantMatch, MethodMatch,
    MixinUsage, MixinUsageKind, VariableTypeKind,
};
pub use queries::namespace_tree::types::{
    IncluderInfo, LibraryNamespaceTree, LibraryPackageTree, LibrarySectionId, LocationInfo,
    MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};
pub use queries::type_query::TypeQuery;
pub use queries::workspace_symbols::types::WorkspaceSymbolMatch;
pub use queries::AnalysisQuery;
pub use resolution::{ConstantRenameTarget, MethodLookupResult};
pub use state::external_facts_template::{
    ProjectNeutralFileFactsSnapshot, ProjectNeutralFileFactsTemplate,
    ProjectNeutralTemplateRejection,
};
pub use state::file_id_map::FileIdMap;
pub use state::{
    AnalysisEngine, AnalysisStats, FileFacts, ResolveMode, ResolvePassStats, SemanticChange,
    SemanticExportFingerprint, SemanticResultFingerprint, SourceFile, SourceFileInput,
    SourceFileSnapshot,
};
