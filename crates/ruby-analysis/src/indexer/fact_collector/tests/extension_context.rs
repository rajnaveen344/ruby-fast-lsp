use crate::core::{FullyQualifiedName, GeneratedOwnerId, NamespaceKind, RubyConstant, SourceKind};
use crate::engine::{AnalysisEngine, SourceFileInput};
use crate::indexer::fact_collector::{
    BlockExecutionContext, FactCollector, FactCollectorExtensionHost,
    NullFactCollectorExtensionHost,
};
use crate::indexer::RubyDocument;
use parking_lot::RwLock;
use ruby_prism::*;
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
        .analysis
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
    assert_eq!(collected.analysis.diagnostics.len(), 1);
    assert_eq!(collected.analysis.diagnostics[0].code, "extension-leaf");
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
            .analysis
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
        .analysis
        .methods
        .iter()
        .find(|fact| fact.fqn.name() == "helper")
        .expect("helper definition must be collected");
    assert_eq!(helper.owner.namespace_parts(), vec![owner]);
    assert!(
        collector.facts.analysis.methods.iter().all(|fact| {
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
