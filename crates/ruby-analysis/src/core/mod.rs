//! Core Ruby analysis data types.
//!
//! This crate intentionally contains no LSP, parser, indexer, or editor
//! dependencies. It is the shared contract for future editor and agent
//! consumers.

pub(crate) mod callable_body;
pub(crate) mod callable_signature;
pub mod constant_type_equation;
pub mod diagnostic_candidate_store;
pub mod diagnostic_store;
pub mod execution_context;
mod file_owned_index;
pub mod fqn_id;
pub mod fully_qualified_name;
pub mod graph_store;
pub mod memory_estimate;
pub mod method_resolution;
pub mod method_return_equation;
pub mod method_store;
pub mod reference_store;
pub mod ruby_method;
pub mod ruby_namespace;
pub mod ruby_type;
pub mod shape_type;
pub mod source_file;
pub mod source_position;
pub mod symbol_store;
pub mod type_inference_outcome;
pub mod type_store;

pub(crate) use callable_body::{
    CallableBodyExpression, CallableBodyParameter, CallableBodyParameterKind, CallableBodySummary,
    ConstantCallableBodyFact,
};
pub(crate) use callable_signature::{
    CallableBlockTemplate, CallableParameterTemplate, CallableSignature, CallableTypeTemplate,
    DirectYieldCall, ForwardedBlockCall,
};
pub use constant_type_equation::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeProjection, ConstantTypeTarget,
};
pub use diagnostic_candidate_store::{
    DiagnosticCandidate, DiagnosticCandidateKind, DiagnosticCandidateStore, RaiseArgCandidate,
};
pub use diagnostic_store::{DiagnosticFact, DiagnosticSeverity, DiagnosticStore};
pub use execution_context::{ExecutionContextFact, ExecutionScopeMode};
pub use fqn_id::{ConstLookupId, FqnId};
pub use fully_qualified_name::{FullyQualifiedName, NamespaceKind};
pub use graph_store::{
    GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact, GraphNodeKind, SemanticGraph,
    StoredGraphEdgeFact, StoredGraphNodeFact, StoredSuperclassResolution,
    StoredUnresolvedGraphEdgeFact, UnresolvedGraphEdgeFact,
};
pub use method_resolution::{MethodCalleeResolution, ResolvedMethodCallee};
pub use method_return_equation::MethodReturnEquation;
pub use method_store::{
    MethodAvailability, MethodFact, MethodParamFact, MethodParamKind, MethodStore,
    MethodVisibilityOverrideFact, StoredMethodFact,
};
pub use reference_store::{
    ConstLookup, ConstantPath, KeywordArgCandidate, MethodCallSignatureCandidate,
    MethodReferenceAccess, MethodReferenceCandidate, MethodReferenceDiagnostics,
    ReferenceCandidate, ReferenceCandidateKind, ReferenceCandidateStore, ReferenceFact,
    ReferenceStore, StoredConstantReferenceCandidate, StoredMethodReferenceCandidate,
    StoredReferenceCandidate, StoredReferenceCandidateKind, StoredReferenceCandidateRef,
    StoredResolvedReferenceCandidate,
};
pub use ruby_method::RubyMethod;
pub use ruby_namespace::{GeneratedOwnerId, RubyConstant};
pub use ruby_type::RubyType;
pub use shape_type::{
    LiteralKey, LiteralValue, ShapeConstructionError, ShapeExactness, ShapeField,
    ShapeFieldPresence, ShapeRest, ShapeStability, ShapeType, MAX_SHAPE_ALIASES, MAX_SHAPE_DEPTH,
    MAX_SHAPE_FIELDS, MAX_SHAPE_SOLVE_ITERATIONS, MAX_SHAPE_UNION_VARIANTS,
};
pub use source_file::{LibraryPackageId, SourceKind};
pub use source_position::{SourcePosition, SourceRange};
pub use symbol_store::{StoredSymbolFact, SymbolFact, SymbolKind, SymbolStore};
pub use type_inference_outcome::{
    InferenceEvidence, InferenceTelemetry, TypeInferenceOutcome, UnknownReason,
};
pub use type_store::{
    SourceFileId, TextRange, TypeFact, TypeProvenance, TypeResolution, TypeStore, TypeSubject,
};
