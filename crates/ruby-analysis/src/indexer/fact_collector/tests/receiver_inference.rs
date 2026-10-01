use crate::core::{
    FileAnalysis, FullyQualifiedName, RubyConstant, RubyType, SourceKind, SymbolFact, SymbolKind,
    TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance, TypeSubject, UnknownReason,
};
use crate::engine::{Project, ResolveMode, SourceFileInput};
use crate::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use crate::indexer::RubyDocument;
use parking_lot::RwLock;
use ruby_prism::*;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

#[test]
fn nested_shape_invalidation_installs_a_fail_closed_local_read() {
    let source = r#"def collect(condition)
  entry = { id: 1 }
  entries = [entry]
  if condition
    dynamic_sink(entry)
  end
  entries
end
"#;
    let uri = Url::parse("file:///workspace/lib/collection.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/collection.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let start = u32::try_from(source.rfind("entries\nend").unwrap()).unwrap();
    let range = TextRange::new(
        file_id,
        start,
        start + u32::try_from("entries".len()).unwrap(),
    );
    assert!(
        collector
            .local_read_type_evidence()
            .iter()
            .any(|(candidate, ruby_type)| *candidate == range
                && *ruby_type == RubyType::Array(vec![RubyType::Unknown])),
        "the safe outer Array constructor remains available internally"
    );
    assert!(
        collector
            .inference_evidence()
            .expression_unknown_reasons
            .contains(&(range, UnknownReason::MutableShapeInvalidated)),
        "the exact expression remains fail-closed for public inference"
    );
}

#[test]
fn direct_expression_range_index_preserves_latest_provenance_and_type_deduplication() {
    let source = "1";
    let uri = Url::parse("file:///workspace/lib/value.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/value.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());
    let program = parse.node().as_program_node().unwrap();
    let node = program.statements().body().iter().next().unwrap();
    let range = collector.direct_range(&node.location());

    collector.direct_push_expression_type(&node, RubyType::string(), TypeProvenance::Runtime);
    collector.direct_push_expression_type(&node, RubyType::integer(), TypeProvenance::Assignment);
    collector.direct_push_expression_type(&node, RubyType::string(), TypeProvenance::Literal);

    assert_eq!(
        collector.facts.expression_indexes[&range].len(),
        2,
        "a repeated type at one range must keep the existing direct-fact deduplication rule"
    );
    assert_eq!(
        collector
            .direct_expression_fact(range, None)
            .map(|fact| fact.ruby_type.clone()),
        Some(RubyType::integer()),
        "the unfiltered lookup must select the latest appended expression fact"
    );
    assert_eq!(
        collector
            .direct_expression_fact(range, Some(TypeProvenance::Runtime))
            .map(|fact| fact.ruby_type.clone()),
        Some(RubyType::string()),
        "a provenance-specific lookup must retain the latest matching fact"
    );
}

#[test]
fn local_receiver_inference_uses_the_active_lexical_scope() {
    let source = "class User\nend\nouter = User.new\n2.times do\n  outer.save\n  inner = \"value\"\n  1.times do\n    inner.upcase\n  end\nend\n";
    let uri = Url::parse("file:///workspace/lib/local_receiver.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/local_receiver.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector = FactCollector::analysis_only(
        document,
        Arc::new(NullFactCollectorExtensionHost),
        engine.clone(),
    );
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let method_owner = |name: &str| {
        collector
            .facts
            .analysis
            .reference_candidates
            .iter()
            .find_map(|candidate| match &candidate.kind {
                crate::core::ReferenceCandidateKind::Method { owner, method, .. }
                    if method.as_str() == name =>
                {
                    Some(owner.iter().map(ToString::to_string).collect::<Vec<_>>())
                }
                crate::core::ReferenceCandidateKind::Constant { .. }
                | crate::core::ReferenceCandidateKind::Method { .. }
                | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
            })
            .unwrap_or_else(|| panic!("expected a method reference candidate for {name}"))
    };

    assert_eq!(method_owner("save"), vec!["User"]);
    assert_eq!(method_owner("upcase"), vec!["String"]);
    assert_eq!(
            collector
                .document
                .variable_scopes()
                .scope_owner_scan_count_for_test(),
            0,
            "fact collection already owns the active lexical scope and must not scan every scope and variable to rediscover it"
        );
}

#[test]
fn ordinary_block_records_unknown_for_an_implicit_receiver() {
    let source = "class Processor\n  def label = \"lexical\"\n  def run\n    configure do\n      label\n    end\n  end\nend\n";
    let uri = Url::parse("file:///workspace/lib/processor.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/processor.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let label_start = u32::try_from(source.rfind("label").unwrap()).unwrap();
    let label_range = TextRange::new(file_id, label_start, label_start + 5);
    assert!(
        collector
            .facts
            .analysis
            .reference_candidates
            .iter()
            .all(|candidate| {
                !matches!(
                    &candidate.kind,
                    crate::core::ReferenceCandidateKind::Method {
                        method,
                        call_expression_range,
                        ..
                    } if method.as_str() == "label" && *call_expression_range == Some(label_range)
                )
            }),
        "an unproven implicit receiver retained a deferred method candidate: {:?}",
        collector.facts.analysis.reference_candidates
    );
    assert_eq!(
        collector
            .expressions
            .call_outcomes
            .get(&label_range)
            .map(TypeInferenceOutcome::unknown_reason),
        Some(Some(UnknownReason::UnknownReceiver))
    );
}

#[test]
fn nested_value_constant_receiver_preserves_its_proven_type() {
    let source = "ARGV.first.upcase\n";
    let uri = Url::parse("file:///workspace/lib/argv.rb").unwrap();
    let mut engine = Project::new();
    let core_file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/embedded/core/constants.rbs"),
        content: "ARGV: Array[String]\n".to_string(),
        kind: SourceKind::Signature,
    });
    let argv = FullyQualifiedName::constant(vec![RubyConstant::new("ARGV").unwrap()]);
    engine.update(
        core_file_id,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                argv.clone(),
                SymbolKind::Constant,
                TextRange::new(core_file_id, 0, 4),
            )],
            types: vec![TypeFact::new(
                TypeSubject::Constant(argv),
                RubyType::array_of(RubyType::string()),
                TextRange::new(core_file_id, 0, 4),
                TypeProvenance::Rbs,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/argv.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let outer_outcome = collector
        .expressions
        .call_outcomes
        .iter()
        .find_map(|(range, outcome)| {
            (range.start_byte == 0 && range.end_byte == 17).then_some(outcome)
        })
        .expect("outer ARGV.first.upcase call must retain a type outcome");
    assert_eq!(
            outer_outcome.clone().into_proven_type(),
            Some(RubyType::string()),
            "nested value constants must use their proven value type instead of a guessed class reference"
        );
}

#[test]
fn immediate_hash_literal_keeps_established_generic_read_methods() {
    let source = "{one: 1}.keys\n";
    let uri = Url::parse("file:///workspace/lib/immediate_hash.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/immediate_hash.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector = FactCollector::analysis_only(
        document,
        Arc::new(NullFactCollectorExtensionHost),
        engine.clone(),
    );
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let range = TextRange::new(file_id, 0, 13);
    let outcome = collector
        .expressions
        .call_outcomes
        .get(&range)
        .expect("the immediate Hash#keys call must retain a proof outcome");
    assert_eq!(
            outcome.clone().into_proven_type(),
            Some(RubyType::array_of(RubyType::symbol())),
            "an immediate Hash literal has no pre-existing alias and may retain its established generic Hash read result"
        );

    engine.write().update(
        file_id,
        FileAnalysis {
            inference: collector.inference_evidence(),
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    let engine = engine.read();
    let query = crate::engine::View::new(&engine);
    assert_eq!(
        query
            .call_expression_outcome_at_position(file_id, 10)
            .and_then(|outcome| outcome.proven_type().cloned()),
        Some(RubyType::array_of(RubyType::symbol())),
        "installing file-owned inference evidence must preserve the immediate Hash#keys proof"
    );
}

#[test]
fn terminal_unknown_receiver_does_not_retain_the_rest_of_a_call_chain() {
    let source = "def normalize(value)\n  value.first.upcase\nend\n";
    let uri = Url::parse("file:///workspace/lib/terminal_unknown.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/terminal_unknown.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let retained = collector
        .facts
        .analysis
        .reference_candidates
        .iter()
        .filter_map(|candidate| match &candidate.kind {
            crate::core::ReferenceCandidateKind::Method { method, .. }
                if matches!(method.as_str(), "first" | "upcase") =>
            {
                Some(method.as_str())
            }
            crate::core::ReferenceCandidateKind::Constant { .. }
            | crate::core::ReferenceCandidateKind::Method { .. }
            | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
        })
        .collect::<Vec<_>>();
    assert!(
            retained.is_empty(),
            "an untyped parameter makes `first` terminal Unknown, so `upcase` cannot become provable after complete graph resolution"
        );
}

#[test]
fn potentially_provable_nested_calls_are_retained_inner_first() {
    let source = "Factory.build.name\nclass Product\n  def name\n    \"value\"\n  end\nend\nclass Factory\n  def self.build\n    Product.new\n  end\nend\n";
    let uri = Url::parse("file:///workspace/lib/deferred_chain.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/deferred_chain.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let retained = collector
        .facts
        .analysis
        .reference_candidates
        .iter()
        .filter_map(|candidate| match &candidate.kind {
            crate::core::ReferenceCandidateKind::Method {
                method,
                call_expression_range,
                ..
            } if matches!(method.as_str(), "build" | "name") => {
                Some((method.as_str(), call_expression_range.is_some()))
            }
            crate::core::ReferenceCandidateKind::Constant { .. }
            | crate::core::ReferenceCandidateKind::Method { .. }
            | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        retained,
        [("build", true), ("name", true)],
        "the inner call must resolve before its retained outer consumer"
    );
}

#[test]
fn local_receiver_inference_does_not_borrow_an_assignment_from_another_method() {
    let source =
        "class User\nend\ndef inspect(user)\n  user.save\nend\ndef build\n  user = User.new\nend\n";
    let uri = Url::parse("file:///workspace/lib/source_order.rb").unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/source_order.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let save_owners = collector
        .facts
        .analysis
        .reference_candidates
        .iter()
        .filter_map(|candidate| match &candidate.kind {
            crate::core::ReferenceCandidateKind::Method { owner, method, .. }
                if method.as_str() == "save" =>
            {
                Some(owner.iter().map(ToString::to_string).collect::<Vec<_>>())
            }
            crate::core::ReferenceCandidateKind::Constant { .. }
            | crate::core::ReferenceCandidateKind::Method { .. }
            | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
            save_owners,
            Vec::<Vec<String>>::new(),
            "an untyped `user` parameter must not borrow `user = User.new` from a different method through a whole-file text scan"
        );
}
