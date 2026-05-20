//! Core Ruby analysis data types.
//!
//! This crate intentionally contains no LSP, parser, indexer, or editor
//! dependencies. It is the shared contract for future editor and agent
//! consumers.

pub mod diagnostic_candidate_store;
pub mod diagnostic_store;
pub mod fully_qualified_name;
pub mod graph_store;
pub mod method_resolution;
pub mod method_store;
pub mod reference_store;
pub mod ruby_method;
pub mod ruby_namespace;
pub mod ruby_type;
pub mod source_file;
pub mod symbol_store;
pub mod type_store;

pub use diagnostic_candidate_store::{
    DiagnosticCandidate, DiagnosticCandidateKind, DiagnosticCandidateStore, RaiseArgCandidate,
};
pub use diagnostic_store::{DiagnosticFact, DiagnosticSeverity, DiagnosticStore};
pub use fully_qualified_name::{FullyQualifiedName, NamespaceKind};
pub use graph_store::{
    GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, GraphStore, UnresolvedGraphEdgeFact,
};
pub use method_resolution::{MethodCalleeResolution, ResolvedMethodCallee};
pub use method_store::{MethodFact, MethodParamFact, MethodParamKind, MethodStore};
pub use reference_store::{
    KeywordArgCandidate, MethodCallSignatureCandidate, ReferenceCandidate, ReferenceCandidateKind,
    ReferenceCandidateStore, ReferenceFact, ReferenceStore,
};
pub use ruby_method::RubyMethod;
pub use ruby_namespace::RubyConstant;
pub use ruby_type::RubyType;
pub use source_file::SourceKind;
pub use symbol_store::{SymbolFact, SymbolKind, SymbolStore};
pub use type_store::{
    SourceFileId, TextRange, TypeFact, TypeProvenance, TypeResolution, TypeStore, TypeSubject,
};
