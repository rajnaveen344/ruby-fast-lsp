//! Editor-agnostic Ruby analysis engine.
//!
//! [`Project`] owns one project's semantic state. Callers register source,
//! replace a file's [`FileAnalysis`](crate::core::FileAnalysis), and read domain results through [`View`].
//!
//! Resolution coordinates the constant and method-return equation solvers in
//! [`crate::inference`], then stores their outcomes through the same file-owned
//! lifecycle. The engine owns lookup policy and solved state; inference owns
//! type rules. Parsing, scheduling, and editor protocol conversion stay outside
//! this module. Stores and their compact representations are internal.
//!
//! A file walk reads project semantics only through the read-only
//! `inference::semantics::Semantics` trait; `semantics.rs` holds the engine's
//! implementations. Any new mid-walk read must become an equation or be added
//! to `Semantics` with a reason.

mod debug;
mod diagnostics;
pub mod lookup;
mod persist;
mod queries;
mod resolution;
mod semantics;
mod state;

pub use debug::reference_storage_sizes;
pub use debug::types::{ExportGraphResponse, LookupResponse};
pub use diagnostics::policy::UNRESOLVED_REQUIRE_CODE;
pub use persist::external_facts_template::{
    ProjectNeutralFileFactsSnapshot, ProjectNeutralFileFactsTemplate,
    ProjectNeutralTemplateRejection,
};
pub use persist::fingerprint::{
    SemanticChange, SemanticExportFingerprint, SemanticResultFingerprint,
};
pub use queries::cache::AnalysisQueryCache;
pub use queries::completion;
pub use queries::hierarchy::types::{
    CallHierarchyMethod, IncomingCall, OutgoingCall, TypeHierarchyEntry, TypeHierarchyNode,
    TypeHierarchyRelation,
};
pub use queries::lookup::types::{
    ConstantHover, ConstantHoverKind, ConstantLookupRequest, ConstantMatch, MethodMatch,
    MixinUsage, MixinUsageKind,
};
pub use queries::namespace_tree::types::{
    IncluderInfo, LibraryNamespaceTree, LibraryPackageTree, LibrarySectionId, LocationInfo,
    MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};
pub use queries::workspace_symbols::types::WorkspaceSymbolMatch;
pub use queries::View;
pub use resolution::{ConstantRenameTarget, MethodLookupResult};
pub use state::{
    AnalysisStat, Project, ResolveMode, ResolveStat, SourceFile, SourceFileInput,
    SourceFileSnapshot,
};

/// Former name of [`Project`]; kept while callers migrate.
pub type AnalysisEngine = Project;

/// Former name of [`View`]; kept while callers migrate.
pub type AnalysisQuery<'a> = View<'a>;
