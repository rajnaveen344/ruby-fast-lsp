//! Shared Ruby names, source coordinates, types, and semantic fact contracts.
//!
//! Import domain records directly from this module. Implementation modules,
//! stores, interned IDs, and stored representations stay inside the crate.
//! These primitives do not traverse source or own semantic query policy.

pub(crate) mod callables;
pub(crate) mod equations;
pub(crate) mod method_resolution;
pub(crate) mod names;
pub(crate) mod source;
pub(crate) mod storage;
pub(crate) mod types;

// Public domain contracts.
pub use equations::constant_type_equation::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeProjection, ConstantTypeTarget,
};
pub use equations::method_return_equation::MethodReturnEquation;
pub use method_resolution::{MethodCalleeResolution, ResolvedMethodCallee};
pub use names::fully_qualified_name::{FqnParts, FullyQualifiedName, NamespaceKind};
pub use names::ruby_method::RubyMethod;
pub use names::ruby_namespace::{GeneratedOwnerId, RubyConstant};
pub use source::execution_context::{ExecutionContextFact, ExecutionScopeMode};
pub use source::source_file::{LibraryPackageId, SourceKind};
pub use source::source_position::{SourcePosition, SourceRange};
pub use storage::diagnostic_candidate_store::{
    DiagnosticCandidate, DiagnosticCandidateKind, RaiseArgCandidate,
};
pub use storage::diagnostic_store::{DiagnosticFact, DiagnosticSeverity};
pub use storage::graph_store::{
    GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact, GraphNodeKind,
    UnresolvedGraphEdgeFact,
};
pub use storage::method_store::{
    MethodAvailability, MethodFact, MethodParamFact, MethodParamKind, MethodVisibility,
    MethodVisibilityOverrideFact,
};
pub use storage::reference_store::{
    ConstantPath, KeywordArgCandidate, MethodCallSignatureCandidate, MethodReferenceAccess,
    MethodReferenceCandidate, MethodReferenceDiagnostics, ReferenceCandidate,
    ReferenceCandidateKind, ReferenceFact,
};
pub use storage::symbol_store::{SymbolFact, SymbolKind};
pub use storage::type_store::{
    SourceFileId, TextRange, TypeFact, TypeProvenance, TypeResolution, TypeSubject,
};
pub use types::ruby_type::RubyType;
pub use types::shape_type::{
    LiteralKey, LiteralValue, ShapeConstructionError, ShapeExactness, ShapeField,
    ShapeFieldPresence, ShapeRest, ShapeStability, ShapeType, MAX_SHAPE_ALIASES, MAX_SHAPE_DEPTH,
    MAX_SHAPE_FIELDS, MAX_SHAPE_SOLVE_ITERATIONS, MAX_SHAPE_UNION_VARIANTS,
};
pub use types::type_inference_outcome::{
    InferenceEvidence, InferenceTelemetry, TypeInferenceOutcome, UnknownReason,
};

// Shared implementation primitives; never part of the consumer API.
pub(crate) use callables::callable_body::{
    CallableBodyExpression, CallableBodyParameter, CallableBodyParameterKind, CallableBodySummary,
    ConstantCallableBodyFact,
};
pub(crate) use callables::callable_signature::{
    CallableBlockTemplate, CallableParameterTemplate, CallableSignature, CallableTypeTemplate,
    DirectYieldCall, ForwardedBlockCall,
};
pub(crate) use names::fqn_id::{ConstLookupId, FqnId};
pub(crate) use storage::diagnostic_candidate_store::DiagnosticCandidateStore;
pub(crate) use storage::diagnostic_store::DiagnosticStore;
pub(crate) use storage::graph_store::{
    SemanticGraph, StoredGraphEdgeFact, StoredGraphNodeFact, StoredSuperclassResolution,
    StoredUnresolvedGraphEdgeFact,
};
pub(crate) use storage::method_store::{MethodStore, StoredMethodFact};
pub(crate) use storage::reference_store::{
    ConstLookup, ReferenceCandidateStore, ReferenceStore, StoredMethodReferenceCandidate,
    StoredReferenceCandidate, StoredReferenceCandidateKind, StoredReferenceCandidateRef,
};
pub(crate) use storage::symbol_store::{StoredSymbolFact, SymbolStore};
pub(crate) use storage::type_store::TypeStore;
