//! Shared Ruby names, source coordinates, types, and semantic fact contracts.
//!
//! Import domain records directly from this module. Implementation modules,
//! stores, interned IDs, and stored representations stay inside the crate.
//! These primitives do not traverse source or own semantic query policy.

pub(crate) mod callable_body;
pub(crate) mod callable_signature;
pub(crate) mod constant_type_equation;
pub(crate) mod diagnostic_candidate_store;
pub(crate) mod diagnostic_store;
pub(crate) mod execution_context;
mod file_owned_index;
pub(crate) mod fqn_id;
pub(crate) mod fully_qualified_name;
pub(crate) mod graph_store;
pub(crate) mod memory_estimate;
pub(crate) mod method_resolution;
pub(crate) mod method_return_equation;
pub(crate) mod method_store;
pub(crate) mod reference_store;
pub(crate) mod ruby_method;
pub(crate) mod ruby_namespace;
pub(crate) mod ruby_type;
pub(crate) mod shape_type;
pub(crate) mod source_file;
pub(crate) mod source_position;
pub(crate) mod symbol_store;
pub(crate) mod type_inference_outcome;
pub(crate) mod type_store;

// Public domain contracts.
pub use constant_type_equation::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeProjection, ConstantTypeTarget,
};
pub use diagnostic_candidate_store::{
    DiagnosticCandidate, DiagnosticCandidateKind, RaiseArgCandidate,
};
pub use diagnostic_store::{DiagnosticFact, DiagnosticSeverity};
pub use execution_context::{ExecutionContextFact, ExecutionScopeMode};
pub use fully_qualified_name::{FqnParts, FullyQualifiedName, NamespaceKind};
pub use graph_store::{
    GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact, GraphNodeKind,
    UnresolvedGraphEdgeFact,
};
pub use method_resolution::{MethodCalleeResolution, ResolvedMethodCallee};
pub use method_return_equation::MethodReturnEquation;
pub use method_store::{
    MethodAvailability, MethodFact, MethodParamFact, MethodParamKind, MethodVisibility,
    MethodVisibilityOverrideFact,
};
pub use reference_store::{
    ConstantPath, KeywordArgCandidate, MethodCallSignatureCandidate, MethodReferenceAccess,
    MethodReferenceCandidate, MethodReferenceDiagnostics, ReferenceCandidate,
    ReferenceCandidateKind, ReferenceFact,
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
pub use symbol_store::{SymbolFact, SymbolKind};
pub use type_inference_outcome::{
    InferenceEvidence, InferenceTelemetry, TypeInferenceOutcome, UnknownReason,
};
pub use type_store::{
    SourceFileId, TextRange, TypeFact, TypeProvenance, TypeResolution, TypeSubject,
};

// Shared implementation primitives; never part of the consumer API.
pub(crate) use callable_body::{
    CallableBodyExpression, CallableBodyParameter, CallableBodyParameterKind, CallableBodySummary,
    ConstantCallableBodyFact,
};
pub(crate) use callable_signature::{
    CallableBlockTemplate, CallableParameterTemplate, CallableSignature, CallableTypeTemplate,
    DirectYieldCall, ForwardedBlockCall,
};
pub(crate) use diagnostic_candidate_store::DiagnosticCandidateStore;
pub(crate) use diagnostic_store::DiagnosticStore;
pub(crate) use fqn_id::{ConstLookupId, FqnId};
pub(crate) use graph_store::{
    SemanticGraph, StoredGraphEdgeFact, StoredGraphNodeFact, StoredSuperclassResolution,
    StoredUnresolvedGraphEdgeFact,
};
pub(crate) use method_store::{MethodStore, StoredMethodFact};
pub(crate) use reference_store::{
    ConstLookup, ReferenceCandidateStore, ReferenceStore, StoredMethodReferenceCandidate,
    StoredReferenceCandidate, StoredReferenceCandidateKind, StoredReferenceCandidateRef,
};
pub(crate) use symbol_store::{StoredSymbolFact, SymbolStore};
pub(crate) use type_store::TypeStore;
