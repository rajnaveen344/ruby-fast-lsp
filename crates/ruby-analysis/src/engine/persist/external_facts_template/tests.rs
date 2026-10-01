use super::type_codec::{restore_ruby_type, snapshot_ruby_type};
use super::{ProjectNeutralFileFactsSnapshot, SnapshotCallableTypeTemplate};
use crate::core::callables::callable_body::CallableBodyExpression;
use crate::core::callables::callable_body::CallableBodyParameter;
use crate::core::callables::callable_body::CallableBodyParameterKind;
use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::callables::callable_body::ConstantCallableBodyFact;
use crate::core::MethodVisibility;
use crate::core::{
    DiagnosticCandidate, DiagnosticCandidateKind, FileAnalysis, FullyQualifiedName, GraphEdgeFact,
    GraphEdgeKind, GraphNodeFact, GraphNodeKind, InferenceEvidence, LiteralKey, LiteralValue,
    MethodFact, MethodVisibilityOverrideFact, RubyConstant, RubyMethod, RubyType, ShapeExactness,
    ShapeField, ShapeRest, ShapeStability, ShapeType, SourceFileId, SourceKind, SymbolFact,
    SymbolKind, TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use crate::engine::{
    AnalysisEngine, AnalysisQuery, ProjectNeutralFileFactsTemplate,
    ProjectNeutralTemplateRejection, ResolveMode, SourceFileInput,
};
use std::path::PathBuf;

fn namespace(name: &str) -> FullyQualifiedName {
    FullyQualifiedName::namespace(vec![RubyConstant::new(name).unwrap()])
}

#[test]
fn persistent_snapshot_round_trips_canonical_shape_and_literal_types() {
    let shape = ShapeType::try_new(
        [
            ShapeField::required(
                LiteralKey::symbol("kind"),
                RubyType::Literal(Box::new(LiteralValue::symbol("ready"))),
            ),
            ShapeField::optional(LiteralKey::string("name"), RubyType::string()),
        ],
        Some(ShapeRest::new(RubyType::symbol(), RubyType::integer())),
        ShapeExactness::Open,
        ShapeStability::Frozen,
    )
    .unwrap();
    let original = RubyType::Shape(Box::new(shape));
    let snapshot = snapshot_ruby_type(&original).unwrap();
    assert_eq!(restore_ruby_type(snapshot, 0).unwrap(), original);
}

#[test]
fn persistent_snapshot_round_trips_capture_free_callable_constant() {
    let source = SourceFileId(41);
    let target = SourceFileId(7);
    let range = TextRange::new(source, 3, 29);
    let constant = FullyQualifiedName::constant(vec![RubyConstant::new("CONVERT").unwrap()]);
    let summary = CallableBodySummary {
        strict_arity: true,
        parameters: vec![CallableBodyParameter {
            name: "value".to_string(),
            kind: CallableBodyParameterKind::Required,
            default: None,
        }],
        captures: Vec::new(),
        result: CallableBodyExpression::Parameter(0),
        node_count: 1,
    };
    let template = ProjectNeutralFileFactsTemplate::try_new(
        source,
        FileAnalysis {
            inference: InferenceEvidence {
                constant_callable_bodies: vec![ConstantCallableBodyFact {
                    constant: constant.clone(),
                    summary: summary.clone(),
                    range,
                }],
                ..InferenceEvidence::default()
            },
            ..FileAnalysis::default()
        },
    )
    .unwrap();

    let snapshot = template.to_persistent_snapshot().unwrap();
    let encoded = postcard::to_allocvec(&snapshot).unwrap();
    let decoded: ProjectNeutralFileFactsSnapshot = postcard::from_bytes(&encoded).unwrap();
    let restored = ProjectNeutralFileFactsTemplate::try_from_persistent_snapshot(decoded).unwrap();
    let facts = restored.instantiate(target);
    assert_eq!(facts.inference.constant_callable_bodies.len(), 1);
    let fact = &facts.inference.constant_callable_bodies[0];
    assert_eq!(fact.constant, constant);
    assert_eq!(fact.summary, summary);
    assert_eq!(fact.range.file_id, target);
}

#[test]
fn persistent_callable_template_uses_postcard_compatible_enum_encoding() {
    let template = SnapshotCallableTypeTemplate::Array(Box::new(
        SnapshotCallableTypeTemplate::Variable("element".to_string()),
    ));
    let encoded = postcard::to_allocvec(&template).unwrap();
    let decoded: SnapshotCallableTypeTemplate = postcard::from_bytes(&encoded).unwrap();
    let SnapshotCallableTypeTemplate::Array(element) = decoded else {
        unreachable_invariant!(
            what = "callable template changed variant in a Postcard round-trip",
            why = "dependency products must restore exact signatures",
            fix = "keep the DTO externally tagged and migrate wire changes explicitly",
        );
    };
    assert!(matches!(
        *element,
        SnapshotCallableTypeTemplate::Variable(ref name) if name == "element"
    ));
}

fn assert_range_file(range: TextRange, expected: SourceFileId) {
    assert_eq!(range.file_id, expected);
}

#[test]
fn template_rebinds_declarations_and_drops_file_local_evidence() {
    let source = SourceFileId(41);
    let target = SourceFileId(7);
    let owner = namespace("Widget");
    let method = RubyMethod::new("call").unwrap();
    let method_fqn = FullyQualifiedName::method(owner.namespace_parts(), method);
    let range = TextRange::new(source, 1, 20);
    let expression = TextRange::new(source, 8, 12);

    let template = ProjectNeutralFileFactsTemplate::try_new(
        source,
        FileAnalysis {
            symbols: vec![
                SymbolFact::new(owner.clone(), SymbolKind::Class, range)
                    .with_name_range(TextRange::new(source, 1, 7)),
                SymbolFact::new(owner.clone(), SymbolKind::LocalVariable, expression),
            ],
            methods: vec![MethodFact::new(method_fqn.clone(), owner.clone(), range)
                .with_name_range(TextRange::new(source, 8, 12))],
            method_visibility_overrides: vec![MethodVisibilityOverrideFact::new(
                owner.clone(),
                method,
                MethodVisibility::Private,
                expression,
            )],
            types: vec![
                TypeFact::new(
                    TypeSubject::Expression(expression),
                    RubyType::string(),
                    expression,
                    TypeProvenance::Inferred,
                ),
                TypeFact::new(
                    TypeSubject::MethodReturn(method_fqn),
                    RubyType::string(),
                    range,
                    TypeProvenance::Inferred,
                ),
            ],
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                range,
            )],
            graph_edges: vec![GraphEdgeFact::new(
                owner.clone(),
                namespace("Object"),
                GraphEdgeKind::Superclass,
                range,
            )],
            unresolved_graph_edges: vec![UnresolvedGraphEdgeFact::new(
                owner.clone(),
                vec![RubyConstant::new("Enumerable").unwrap()],
                false,
                owner,
                GraphEdgeKind::Include,
                range,
            )],
            inference: InferenceEvidence {
                call_expression_outcomes: vec![(
                    expression,
                    TypeInferenceOutcome::proven(RubyType::string()),
                )],
                ..Default::default()
            },
            local_read_types: vec![(expression, RubyType::string())].into_boxed_slice(),
            ..FileAnalysis::default()
        },
    )
    .unwrap();

    let snapshot = template.to_persistent_snapshot().unwrap();
    let restored = ProjectNeutralFileFactsTemplate::try_from_persistent_snapshot(snapshot).unwrap();
    let facts = restored.instantiate(target);
    assert_eq!(facts.symbols.len(), 1);
    assert_range_file(facts.symbols[0].range, target);
    assert_range_file(facts.symbols[0].name_range, target);
    assert_range_file(facts.methods[0].range, target);
    assert_range_file(facts.methods[0].name_range, target);
    assert_range_file(facts.method_visibility_overrides[0].range, target);
    assert_eq!(facts.types.len(), 1);
    assert_range_file(facts.types[0].range, target);
    let TypeSubject::MethodReturn(_) = facts.types[0].subject else {
        panic!("expected retained method-return type subject");
    };
    assert_eq!(facts.inference, InferenceEvidence::default());
    assert!(facts.local_read_types.is_empty());
    assert_range_file(facts.graph_nodes[0].range, target);
    assert_range_file(facts.graph_edges[0].range, target);
    assert_range_file(facts.unresolved_graph_edges[0].range, target);
}

#[test]
fn template_rejects_project_specific_candidates() {
    let source = SourceFileId(1);
    let rejection = ProjectNeutralFileFactsTemplate::try_new(
        source,
        FileAnalysis {
            diagnostic_candidates: vec![DiagnosticCandidate::new(
                TextRange::new(source, 0, 1),
                DiagnosticCandidateKind::BadSplat {
                    operator: "*".to_string(),
                    arg_repr: "value".to_string(),
                    expected: "Array".to_string(),
                },
            )],
            ..FileAnalysis::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        rejection,
        ProjectNeutralTemplateRejection::ProjectSpecificFacts
    );
}

#[test]
fn rebound_templates_preserve_navigation_without_sharing_file_identity() {
    let template_file = SourceFileId(99);
    let owner = namespace("CachedWidget");
    let declaration = TextRange::new(template_file, 0, 12);
    let template = ProjectNeutralFileFactsTemplate::try_new(
        template_file,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                owner.clone(),
                SymbolKind::Class,
                declaration,
            )],
            graph_nodes: vec![GraphNodeFact::new(owner, GraphNodeKind::Class, declaration)],
            ..FileAnalysis::default()
        },
    )
    .unwrap();

    let mut first = AnalysisEngine::new();
    let first_file = first.register_file(SourceFileInput {
        path: PathBuf::from("/cache/a/cached_widget.rb"),
        content: "class CachedWidget; end".to_string(),
        kind: SourceKind::Gem,
    });
    first.replace_facts(
        first_file,
        template.instantiate(first_file),
        ResolveMode::Immediate,
    );

    let mut second = AnalysisEngine::new();
    second.register_file(SourceFileInput {
        path: PathBuf::from("/other/preexisting.rb"),
        content: String::new(),
        kind: SourceKind::Project,
    });
    let second_file = second.register_file(SourceFileInput {
        path: PathBuf::from("/cache/b/cached_widget.rb"),
        content: "class CachedWidget; end".to_string(),
        kind: SourceKind::Gem,
    });
    second.replace_facts(
        second_file,
        template.instantiate(second_file),
        ResolveMode::Immediate,
    );

    let parts = [RubyConstant::new("CachedWidget").unwrap()];
    assert_eq!(
        AnalysisQuery::new(&first).constant_definition_ranges(&parts, &[]),
        vec![TextRange::new(first_file, 0, 12)]
    );
    assert_eq!(
        AnalysisQuery::new(&second).constant_definition_ranges(&parts, &[]),
        vec![TextRange::new(second_file, 0, 12)]
    );
    assert_ne!(first_file, second_file);
}
