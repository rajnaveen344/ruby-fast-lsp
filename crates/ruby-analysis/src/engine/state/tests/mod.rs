use crate::core::{
    DiagnosticFact, DiagnosticSeverity, FileAnalysis, FullyQualifiedName, GeneratedOwnerId,
    GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact, GraphNodeKind,
    InferenceEvidence, InferenceTelemetry, MethodCalleeResolution, MethodFact,
    MethodReturnEquation, NamespaceKind, ReferenceCandidate, RubyConstant, RubyMethod, RubyType,
    SymbolFact, SymbolKind, TypeFact, TypeInferenceOutcome, TypeProvenance, TypeSubject,
    UnknownReason, UnresolvedGraphEdgeFact,
};

use super::*;
use crate::core::{SourceFileId, SourceKind, TextRange, TypeResolution};
use crate::engine::lookup::{self, LookupReceiver, MethodAnswer, MethodRequest, MethodWant};
use crate::engine::persist::fingerprint::SemanticChange;
use crate::engine::resolution::{
    method_lookup_chain, method_lookup_chain_for_reference_cached,
    method_lookup_chain_uncached_construction_count, namespace_target_exists,
    MethodLookupChainCache,
};
use crate::engine::AnalysisQueryCache;
use crate::engine::ConstantLookupRequest;
use crate::engine::View;
use crate::inference::semantics::ReceiverAccess;
use std::path::PathBuf;

mod caches;
mod constants;
mod fingerprints;
mod graph;
mod inference_outcomes;
mod lifecycle;
mod navigation;
mod remove;

/// Ask `view` for `want` of `method` on the `owner` namespace with `access`.
fn ask(
    view: &View<'_>,
    owner: &FullyQualifiedName,
    method: &RubyMethod,
    access: ReceiverAccess<'_>,
    want: MethodWant,
) -> MethodAnswer {
    let request = MethodRequest::new(LookupReceiver::Namespace(owner), *method, want);
    lookup::method(view, request.with_access(access))
}

/// [`ask`] with an unrestricted receiver.
fn ask_any(
    view: &View<'_>,
    owner: &FullyQualifiedName,
    method: &RubyMethod,
    want: MethodWant,
) -> MethodAnswer {
    ask(view, owner, method, ReceiverAccess::Any, want)
}

fn constant_subject(name: &str) -> TypeSubject {
    TypeSubject::Constant(FullyQualifiedName::constant(vec![
        RubyConstant::new(name).unwrap()
    ]))
}

fn register_project_file(
    engine: &mut Project,
    path: impl Into<std::path::PathBuf>,
    source: impl Into<String>,
) -> SourceFileId {
    engine.register_file(SourceFileInput {
        path: path.into(),
        content: source.into(),
        kind: SourceKind::Project,
    })
}

fn explicit_method_call_candidate(
    method_range: TextRange,
    call_range: TextRange,
    owner: Vec<RubyConstant>,
    owner_kind: NamespaceKind,
    method: RubyMethod,
    receiver_expression_range: Option<TextRange>,
) -> ReferenceCandidate {
    ReferenceCandidate::method(
        method_range,
        crate::core::MethodReferenceCandidate {
            owner,
            owner_kind,
            method,
            is_super: false,
            access: crate::core::MethodReferenceAccess::ExplicitReceiver,
            caller: None,
            call_expression_range: Some(call_range),
            preferred_definition_range: None,
            diagnostics: crate::core::MethodReferenceDiagnostics {
                diagnostic_range: method_range,
                receiver_label: None,
                receiver_expression_range,
                receiver_type: None,
                diagnose_unresolved: true,
                allow_unindexed_owner: false,
                safe_navigation: false,
                signature: Some(crate::core::MethodCallSignatureCandidate::default()),
            },
        },
    )
}
