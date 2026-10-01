use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeFact, GraphNodeKind, RubyConstant, RubyType,
    SourceKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
};
use crate::engine::{AnalysisEngine, FileFacts, ResolveMode, SourceFileInput};
use crate::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use crate::indexer::RubyDocument;
use parking_lot::RwLock;
use ruby_prism::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

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
            .analysis
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
            .analysis
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
            collector.facts.analysis
                .graph_nodes
                .iter()
                .any(|fact| fact.fqn == platform_app && fact.kind == GraphNodeKind::Class),
            "recollecting class PlatformApp while its ClassReference remains visible must still emit the class graph node; nodes={:?}",
            collector.facts.analysis
                .graph_nodes
                .iter()
                .map(|fact| fact.fqn.to_string())
                .collect::<Vec<_>>()
        );
    assert!(
        collector
            .facts
            .analysis
            .graph_edges
            .iter()
            .any(|edge| { edge.kind == GraphEdgeKind::Superclass && edge.source == platform_app })
            || collector
                .facts
                .analysis
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
        .analysis
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
        collector.facts.analysis.graph_nodes.iter().all(|fact| {
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
        .analysis
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
            .analysis
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
            .analysis
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
            .analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        vec!["cyclic-inheritance", "conflicting-superclass"]
    );
    assert_eq!(
        collector.facts.analysis.graph_edges.len(),
        2,
        "only the two valid ancestry edges may become same-pass semantic input"
    );
}
