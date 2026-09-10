use super::{
    BlockExecutionContext, FactCollector, FactCollectorExtensionHost,
    NullFactCollectorExtensionHost,
};
use crate::core::{
    FullyQualifiedName, GeneratedOwnerId, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
    NamespaceKind, RubyConstant, RubyType, SourceKind, SymbolFact, SymbolKind, TextRange, TypeFact,
    TypeInferenceOutcome, TypeProvenance, TypeSubject, UnknownReason,
};
use crate::engine::{AnalysisEngine, FileFacts, ResolveMode, SourceFileInput};
use crate::indexer::RubyDocument;
use parking_lot::RwLock;
use ruby_prism::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

#[derive(Debug)]
struct SyntheticExecutionContextHost {
    owner: RubyConstant,
}

#[test]
fn nested_extension_calls_preserve_parent_context_and_handled_decisions() {
    #[derive(Debug, Default)]
    struct RecordingHost {
        calls: parking_lot::Mutex<Vec<(String, Vec<String>)>>,
    }

    impl FactCollectorExtensionHost for RecordingHost {
        fn process_call_node(&self, collector: &mut FactCollector, node: &CallNode) -> bool {
            let name = String::from_utf8_lossy(node.name().as_slice()).into_owned();
            let parents = collector
                .enclosing_extension_calls()
                .iter()
                .map(|call| call.method_name.clone())
                .collect();
            self.calls.lock().push((name.clone(), parents));
            if name == "leaf" {
                collector.push_warning_diagnostic(
                    collector
                        .document()
                        .prism_location_to_text_range(&node.location()),
                    "extension-leaf",
                    "Leaf visited inside its enclosing extension calls".to_string(),
                );
            }
            matches!(name.as_str(), "outer" | "child")
        }

        fn should_track_enclosing_call(&self, _collector: &FactCollector, node: &CallNode) -> bool {
            matches!(node.name().as_slice(), b"outer" | b"inner")
        }
    }

    let source = "outer(supplied.child, untracked(inner(leaf)), sibling)\nafter\n";
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/nested.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let document = RubyDocument::with_analysis_file_id(
        Url::parse("file:///workspace/lib/nested.rb").unwrap(),
        source.to_string(),
        0,
        file_id,
    );
    let host = Arc::new(RecordingHost::default());
    let mut collector =
        FactCollector::analysis_only(document, host.clone(), Arc::new(RwLock::new(engine)));
    let parse = ruby_prism::parse(source.as_bytes());
    collector.visit(&parse.node());

    let observed = host.calls.lock();
    let expected = [
        ("outer", vec![]),
        ("child", vec!["outer"]),
        ("supplied", vec!["outer"]),
        ("untracked", vec!["outer"]),
        ("inner", vec!["outer"]),
        ("leaf", vec!["outer", "inner"]),
        ("sibling", vec!["outer"]),
        ("after", vec![]),
    ];
    let observed = observed
        .iter()
        .map(|(name, parents)| {
            (
                name.as_str(),
                parents.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, expected);
    assert!(collector.enclosing_extension_calls().is_empty());

    let collected = collector.finish();
    let methods = collected
        .reference_candidates
        .iter()
        .filter_map(|candidate| match &candidate.kind {
            crate::core::ReferenceCandidateKind::Method { method, .. } => Some(method.as_str()),
            crate::core::ReferenceCandidateKind::Constant { .. }
            | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !methods.contains(&"child"),
        "a handled call stays handled after its receiver exits"
    );
    assert!(
        methods.contains(&"sibling"),
        "a handled parent must not suppress its child candidates"
    );
    assert!(
        methods.contains(&"after"),
        "extension context must not leak into the next statement"
    );
    assert_eq!(collected.diagnostics.len(), 1);
    assert_eq!(collected.diagnostics[0].code, "extension-leaf");
}

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
    let mut engine = AnalysisEngine::new();
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
    let mut engine = AnalysisEngine::new();
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

impl FactCollectorExtensionHost for SyntheticExecutionContextHost {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) -> bool {
        if node.name().as_slice() != b"describe" {
            return false;
        }
        let block = node.block().expect("test describe call must have a block");
        visitor.set_pending_block_execution_context(BlockExecutionContext {
            block_range: visitor.direct_range(&block.location()),
            implicit_receiver: vec![self.owner],
            implicit_receiver_kind: NamespaceKind::Instance,
            method_definition_owner: vec![self.owner],
            method_definition_kind: NamespaceKind::Instance,
        });
        true
    }
}

#[test]
fn attr_macros_use_the_method_definition_context_in_direct_facts() {
    let source =
        "class User\n  attr_accessor :name\n  class << self\n    attr_reader :count\n  end\nend\n";
    let uri = Url::parse("file:///workspace/lib/user.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/user.rb"),
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

    let user = vec![RubyConstant::new("User").unwrap()];
    let owner_for = |name: &str| {
        collector
            .facts
            .direct
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == name)
            .unwrap_or_else(|| panic!("expected direct attr method `{name}`"))
            .owner
            .clone()
    };
    assert_eq!(
        owner_for("name"),
        FullyQualifiedName::namespace_with_kind(user.clone(), NamespaceKind::Instance),
        "an ordinary class-body attr reader must be instance-owned"
    );
    assert_eq!(
        owner_for("name="),
        FullyQualifiedName::namespace_with_kind(user.clone(), NamespaceKind::Instance),
        "an ordinary class-body attr writer must be instance-owned"
    );
    assert_eq!(
        owner_for("count"),
        FullyQualifiedName::namespace_with_kind(user, NamespaceKind::Singleton),
        "an attr reader inside class << self must remain singleton-owned"
    );
}

#[test]
fn extension_context_rehomes_block_method_without_changing_lexical_namespace() {
    let source = "module Lexical\n  describe do\n    def helper\n    end\n    helper\n    VALUE\n  end\nend\n";
    let uri = Url::parse("file:///workspace/spec/context_spec.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/spec/context_spec.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let owner = RubyConstant::generated_owner(
        GeneratedOwnerId::new("test-extension", uri.as_str(), "group:1:2")
            .expect("test generated owner must be valid"),
    );
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector = FactCollector::analysis_only(
        document,
        Arc::new(SyntheticExecutionContextHost { owner }),
        engine,
    );
    let parse = ruby_prism::parse(source.as_bytes());
    collector.visit(&parse.node());

    let helper = collector
        .facts
        .direct
        .methods
        .iter()
        .find(|fact| fact.fqn.name() == "helper")
        .expect("helper definition must be collected");
    assert_eq!(helper.owner.namespace_parts(), vec![owner]);
    assert!(
        collector.facts.direct.methods.iter().all(|fact| {
            fact.fqn.name() != "helper"
                || fact.owner.namespace_parts() != vec![RubyConstant::new("Lexical").unwrap()]
        }),
        "execution-owned method must not also leak onto the lexical module"
    );
    assert_eq!(
        collector.scope_tracker.get_ns_stack(),
        Vec::<RubyConstant>::new(),
        "all lexical and execution frames must be balanced after traversal"
    );
}

#[test]
fn local_receiver_inference_uses_the_active_lexical_scope() {
    let source = "class User\nend\nouter = User.new\n2.times do\n  outer.save\n  inner = \"value\"\n  1.times do\n    inner.upcase\n  end\nend\n";
    let uri = Url::parse("file:///workspace/lib/local_receiver.rb").unwrap();
    let mut engine = AnalysisEngine::new();
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
            .references
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
    let mut engine = AnalysisEngine::new();
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
        collector.facts.references.iter().all(|candidate| {
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
        collector.facts.references
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
    let mut engine = AnalysisEngine::new();
    let core_file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/embedded/core/constants.rbs"),
        content: "ARGV: Array[String]\n".to_string(),
        kind: SourceKind::Signature,
    });
    let argv = FullyQualifiedName::constant(vec![RubyConstant::new("ARGV").unwrap()]);
    engine.replace_facts(
        core_file_id,
        FileFacts {
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
    let mut engine = AnalysisEngine::new();
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

    engine.write().replace_facts(
        file_id,
        FileFacts {
            inference: collector.inference_evidence(),
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    let engine = engine.read();
    let query = crate::engine::AnalysisQuery::new(&engine);
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
    let mut engine = AnalysisEngine::new();
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
        .references
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
    let mut engine = AnalysisEngine::new();
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
        .references
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
    let mut engine = AnalysisEngine::new();
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
        .references
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

#[test]
fn recovered_invalid_namespace_does_not_unbalance_an_enclosing_method_context() {
    let source = "def outer\n  def self.forName(module, name); end\nend\n";
    let uri = Url::parse("file:///workspace/lib/recovered.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/recovered.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    assert_eq!(
        collector.scope_tracker.get_ns_stack(),
        Vec::<RubyConstant>::new()
    );
    assert!(!collector.scope_tracker.execution_context_active());
}

#[test]
fn shared_known_namespaces_are_immutable_while_file_declarations_stay_local() {
    let shared_namespace =
        FullyQualifiedName::namespace(vec![RubyConstant::new("Shared").unwrap()]);
    let local_namespace = FullyQualifiedName::namespace(vec![RubyConstant::new("Local").unwrap()]);
    let shared = Arc::new(HashSet::from([shared_namespace.clone()]));
    let source = "class Local\nend\n";
    let uri = Url::parse("file:///workspace/lib/local.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/local.rb"),
        content: source.to_string(),
        kind: SourceKind::Gem,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri.clone(), source.to_string(), 0, file_id);
    let mut collector = FactCollector::analysis_only(
        document,
        Arc::new(NullFactCollectorExtensionHost),
        engine.clone(),
    )
    .with_shared_direct_known_namespaces(shared.clone());
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    assert_eq!(
        collector.direct_resolve_namespace(&[RubyConstant::new("Shared").unwrap()], true),
        Some(shared_namespace),
        "the immutable batch snapshot must participate in direct lookup"
    );
    assert_eq!(
        collector.direct_resolve_namespace(&[RubyConstant::new("Local").unwrap()], true),
        Some(local_namespace),
        "declarations from the current file must remain directly visible"
    );
    assert_eq!(
        shared.len(),
        1,
        "file-local declarations must not mutate the shared batch snapshot"
    );

    let other_file_id = engine.write().register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/other.rb"),
        content: String::new(),
        kind: SourceKind::Gem,
    });
    let other_document = RubyDocument::with_analysis_file_id(
        Url::parse("file:///workspace/lib/other.rb").unwrap(),
        String::new(),
        0,
        other_file_id,
    );
    let other_collector = FactCollector::analysis_only(
        other_document,
        Arc::new(NullFactCollectorExtensionHost),
        engine,
    )
    .with_shared_direct_known_namespaces(shared);
    assert_eq!(
        other_collector.direct_resolve_namespace(&[RubyConstant::new("Local").unwrap()], true),
        None,
        "one file's declarations must not leak into another file's local overlay"
    );
}

#[test]
fn qualified_class_superclass_uses_predeclaration_lexical_context() {
    let source = "class BigDecimal\n  def to_s\n    \"base\"\n  end\nend\n\nmodule SitemapGenerator\nend\n\nclass SitemapGenerator::BigDecimal < BigDecimal\n  alias_method :original_to_s, :to_s\nend\n";
    let uri = Url::parse("file:///workspace/core_ext/big_decimal.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/core_ext/big_decimal.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let source = FullyQualifiedName::namespace(vec![
        RubyConstant::new("SitemapGenerator").unwrap(),
        RubyConstant::new("BigDecimal").unwrap(),
    ]);
    let target = FullyQualifiedName::namespace(vec![RubyConstant::new("BigDecimal").unwrap()]);
    assert!(
        collector
            .facts
            .direct
            .graph_edges
            .iter()
            .any(|edge| edge.kind == GraphEdgeKind::Superclass
                && edge.source == source
                && edge.target == target),
        "the qualified class must inherit the pre-existing lexical BigDecimal"
    );
    assert!(
        collector
            .facts
            .direct
            .graph_edges
            .iter()
            .all(|edge| edge.kind != GraphEdgeKind::Superclass
                || edge.source != source
                || edge.target != source),
        "declaring the class must not make it its own superclass"
    );
}

#[test]
fn class_reindex_against_existing_class_reference_still_emits_graph_node() {
    let source = "class PlatformApp < Object\nend\n";
    let uri = Url::parse("file:///workspace/lib/api_app.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/api_app.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let platform_app =
        FullyQualifiedName::namespace(vec![RubyConstant::new("PlatformApp").unwrap()]);
    let constant = FullyQualifiedName::constant(vec![RubyConstant::new("PlatformApp").unwrap()]);
    // Prior didOpen / earlier pass left the ordinary class ClassReference in the engine.
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                platform_app.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 5),
            )],
            types: vec![TypeFact::new(
                TypeSubject::Constant(constant.clone()),
                RubyType::ClassReference(constant),
                TextRange::new(file_id, 0, 5),
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());
    collector.visit(&parse.node());

    assert!(
            collector.facts.direct
                .graph_nodes
                .iter()
                .any(|fact| fact.fqn == platform_app && fact.kind == GraphNodeKind::Class),
            "recollecting class PlatformApp while its ClassReference remains visible must still emit the class graph node; nodes={:?}",
            collector.facts.direct
                .graph_nodes
                .iter()
                .map(|fact| fact.fqn.to_string())
                .collect::<Vec<_>>()
        );
    assert!(
        collector
            .facts
            .direct
            .graph_edges
            .iter()
            .any(|edge| { edge.kind == GraphEdgeKind::Superclass && edge.source == platform_app })
            || collector
                .facts
                .direct
                .unresolved_graph_edges
                .iter()
                .any(|edge| {
                    edge.kind == GraphEdgeKind::Superclass && edge.source == platform_app
                }),
        "superclass edge must still be emitted for the class declaration"
    );
}

#[test]
fn class_reopening_through_a_constant_alias_keeps_the_original_owner_identity() {
    let source = "module Types\n\
                          class Original\n\
                          end\n\
                          Alias = Original\n\
                          class Alias\n\
                            def from_alias\n\
                            end\n\
                          end\n\
                        end\n";
    let uri = Url::parse("file:///workspace/lib/constant_alias.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/constant_alias.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let method = collector
        .facts
        .direct
        .methods
        .iter()
        .find(|fact| fact.fqn.name() == "from_alias")
        .expect("method in aliased class reopening must be collected");
    assert_eq!(
        method.owner,
        FullyQualifiedName::namespace(vec![
            RubyConstant::new("Types").unwrap(),
            RubyConstant::new("Original").unwrap(),
        ]),
        "class Alias must reopen the class object stored in Alias"
    );
    assert!(
        collector.facts.direct.graph_nodes.iter().all(|fact| {
            fact.fqn
                != FullyQualifiedName::namespace(vec![
                    RubyConstant::new("Types").unwrap(),
                    RubyConstant::new("Alias").unwrap(),
                ])
        }),
        "a value constant alias must not become a second class identity"
    );
}

#[test]
fn explicit_subclass_does_not_reopen_an_alias_as_its_own_superclass() {
    let source = "class StringScanner\n\
                      end\n\
                      module Sass\n\
                        module Util\n\
                        end\n\
                      end\n\
                      Sass::Util::MultibyteStringScanner = StringScanner\n\
                      class Sass::Util::MultibyteStringScanner < StringScanner\n\
                        def wrapped_string\n\
                          string\n\
                        end\n\
                      end\n";
    let uri = Url::parse("file:///workspace/sass/multibyte_string_scanner.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/sass/multibyte_string_scanner.rb"),
        content: source.to_string(),
        kind: SourceKind::Gem,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let parse = ruby_prism::parse(source.as_bytes());

    collector.visit(&parse.node());

    let subclass = FullyQualifiedName::namespace(vec![
        RubyConstant::new("Sass").unwrap(),
        RubyConstant::new("Util").unwrap(),
        RubyConstant::new("MultibyteStringScanner").unwrap(),
    ]);
    let string_scanner =
        FullyQualifiedName::namespace(vec![RubyConstant::new("StringScanner").unwrap()]);
    let method = collector
        .facts
        .direct
        .methods
        .iter()
        .find(|fact| fact.fqn.name() == "wrapped_string")
        .expect("method in the explicit subclass must be collected");

    assert_eq!(
        method.owner, subclass,
        "an alias cannot reopen its target when that would make the target inherit itself"
    );
    assert!(
        collector
            .facts
            .direct
            .graph_edges
            .iter()
            .any(|edge| edge.kind == GraphEdgeKind::Superclass
                && edge.source == subclass
                && edge.target == string_scanner),
        "the feasible explicit subclass branch must retain its superclass"
    );
    assert!(
        collector
            .facts
            .direct
            .graph_edges
            .iter()
            .all(|edge| edge.kind != GraphEdgeKind::Superclass
                || edge.source != string_scanner
                || edge.target != string_scanner),
        "the flow-insensitive alias must not create StringScanner < StringScanner"
    );
}

#[test]
fn local_graph_edge_validation_rejects_cycles_and_conflicting_superclasses() {
    let source = "";
    let uri = Url::parse("file:///workspace/lib/invalid_inheritance.rb").unwrap();
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/workspace/lib/invalid_inheritance.rb"),
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine);
    let range = TextRange::new(file_id, 0, 0);
    let a = FullyQualifiedName::namespace(vec![RubyConstant::new("A").unwrap()]);
    let b = FullyQualifiedName::namespace(vec![RubyConstant::new("B").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);

    assert!(collector.direct_push_resolved_edge(
        a.clone(),
        b.clone(),
        GraphEdgeKind::Include,
        range,
    ));
    assert!(
        !collector.direct_push_resolved_edge(b.clone(), a.clone(), GraphEdgeKind::Include, range,),
        "the edge that closes a local ancestry cycle must be rejected"
    );
    assert!(collector.direct_push_resolved_edge(
        child.clone(),
        a.clone(),
        GraphEdgeKind::Superclass,
        range,
    ));
    assert!(
        !collector.direct_push_resolved_edge(
            child.clone(),
            b.clone(),
            GraphEdgeKind::Superclass,
            range,
        ),
        "a second distinct local superclass must be rejected"
    );

    assert_eq!(
        collector
            .facts
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        vec!["cyclic-inheritance", "conflicting-superclass"]
    );
    assert_eq!(
        collector.facts.direct.graph_edges.len(),
        2,
        "only the two valid ancestry edges may become same-pass semantic input"
    );
}
