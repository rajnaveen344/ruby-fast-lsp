use crate::core::{
    FileAnalysis, InferenceEvidence, MethodReferenceCandidate, MethodReferenceDiagnostics,
    NamespaceKind, ReferenceCandidate, RubyConstant, RubyMethod, RubyType, SourceFileId,
    SourceKind, TextRange, UnknownReason,
};
use crate::engine::{AnalysisEngine, ResolveMode, SourceFileInput};

fn fixture() -> (
    AnalysisEngine,
    SourceFileId,
    TextRange,
    TextRange,
    FileAnalysis,
) {
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: "receiver.rb".into(),
        content: "x.upcase".to_string(),
        kind: SourceKind::Project,
    });
    let receiver = TextRange::new(file_id, 0, 1);
    let message = TextRange::new(file_id, 2, 8);
    let facts = FileAnalysis {
        reference_candidates: vec![ReferenceCandidate::method(
            message,
            MethodReferenceCandidate {
                owner: vec![RubyConstant::new("NilClass").expect("valid language class")],
                owner_kind: NamespaceKind::Instance,
                method: RubyMethod::new("upcase").expect("valid Ruby method"),
                is_super: false,
                access: crate::core::MethodReferenceAccess::ExplicitReceiver,
                caller: None,
                call_expression_range: None,
                preferred_definition_range: None,
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: message,
                    receiver_label: Some("NilClass".to_string()),
                    receiver_expression_range: Some(receiver),
                    receiver_type: Some(Box::new(RubyType::nil_class())),
                    diagnose_unresolved: true,
                    allow_unindexed_owner: false,
                    safe_navigation: false,
                    signature: None,
                },
            },
        )],
        ..FileAnalysis::default()
    };
    (engine, file_id, receiver, message, facts)
}

#[test]
fn exact_call_receiver_type_keeps_unknown_and_union_proof_barriers() {
    let (mut engine, file_id, receiver, message, facts) = fixture();
    engine.replace_facts(file_id, facts.clone(), ResolveMode::Deferred);
    assert_eq!(
        engine.query().exact_call_receiver_type(message, receiver),
        Some(RubyType::nil_class())
    );

    let unknown = FileAnalysis {
        inference: InferenceEvidence {
            expression_unknown_reasons: vec![(receiver, UnknownReason::UnresolvedAssignmentValue)],
            ..InferenceEvidence::default()
        },
        ..facts.clone()
    };
    engine.replace_facts(file_id, unknown, ResolveMode::Deferred);
    assert_eq!(
        engine.query().exact_call_receiver_type(message, receiver),
        Some(RubyType::Unknown)
    );

    let union = RubyType::union(vec![RubyType::nil_class(), RubyType::string()]);
    engine.replace_facts(
        file_id,
        FileAnalysis {
            local_read_types: vec![(receiver, union.clone())].into_boxed_slice(),
            ..facts
        },
        ResolveMode::Deferred,
    );
    assert_eq!(
        engine.query().exact_call_receiver_type(message, receiver),
        Some(union)
    );
}

#[test]
fn exact_call_receiver_type_rejects_other_message_and_receiver_ranges() {
    let (mut engine, file_id, receiver, message, facts) = fixture();
    engine.replace_facts(file_id, facts, ResolveMode::Deferred);
    assert_eq!(
        engine
            .query()
            .exact_call_receiver_type(TextRange::new(file_id, 2, 7), receiver),
        None
    );
    assert_eq!(
        engine
            .query()
            .exact_call_receiver_type(TextRange::new(file_id, 3, 8), receiver),
        None
    );
    assert_eq!(
        engine
            .query()
            .exact_call_receiver_type(message, TextRange::new(file_id, 0, 2)),
        None
    );
}
