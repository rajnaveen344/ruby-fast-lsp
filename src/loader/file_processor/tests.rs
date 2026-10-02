use super::merge::{merge_execution_context_direct_facts, merge_precise_visitor_type_facts};
use super::*;
use crate::server::RubyLanguageServer;
use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, GraphNodeFact, GraphNodeKind, MethodFact, RubyConstant,
    RubyMethod, RubyType, SourceKind, SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange,
    TypeFact, TypeProvenance, TypeSubject,
};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, ResolveMode, SemanticChange};
use std::collections::HashSet;
use std::sync::Arc;

/// Analyze `source` as the open document `uri` with forced current-file
/// resolution and commit it, as an open-document refresh does.
fn analyze_and_commit(
    processor: &FileProcessor,
    server: &RubyLanguageServer,
    uri: &Url,
    source: &str,
) -> anyhow::Result<ProcessResult> {
    let ctx = server.load_context_for_uri(uri);
    let loaded = processor.analyze_file_current_file_resolution_forced(uri, source, &ctx)?;
    Ok(loaded.commit(&ctx))
}

#[test]
fn file_processor_reports_body_only_and_exported_api_changes() {
    let server = RubyLanguageServer::default();
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let uri = crate::test::harness::fixture_uri("/app/user.rb");

    let initial = analyze_and_commit(
        &processor,
        &server,
        &uri,
        "class User\n  def name\n    'A'\n  end\nend\n",
    )
    .unwrap();
    assert_eq!(initial.semantic_change, SemanticChange::InitialIndex);

    let body = analyze_and_commit(
        &processor,
        &server,
        &uri,
        "class User\n  def name\n    'B'\n  end\nend\n",
    )
    .unwrap();
    assert_eq!(body.semantic_change, SemanticChange::BodyOnly);

    let api = analyze_and_commit(
        &processor,
        &server,
        &uri,
        "class User\n  def name(prefix)\n    prefix\n  end\nend\n",
    )
    .unwrap();
    assert_eq!(api.semantic_change, SemanticChange::ExportsChanged);
}

#[test]
fn execution_context_merge_replaces_lexical_method_with_generated_owner() {
    let file_id = ruby_analysis::core::SourceFileId(7);
    let range = TextRange::new(file_id, 20, 44);
    let method = RubyMethod::new("helper").unwrap();
    let lexical_parts = vec![RubyConstant::new("Lexical").unwrap()];
    let generated_part = RubyConstant::generated_owner(
        ruby_analysis::core::GeneratedOwnerId::new(
            "rspec-ruby",
            "file:///workspace/spec/example_spec.rb",
            "group:1:2",
        )
        .unwrap(),
    );
    let generated_parts = vec![generated_part];
    let lexical_fqn = FullyQualifiedName::method(lexical_parts.clone(), method);
    let generated_fqn = FullyQualifiedName::method(generated_parts.clone(), method);
    let mut merged = ruby_analysis::core::FileAnalysis {
        methods: vec![MethodFact::new(
            lexical_fqn.clone(),
            FullyQualifiedName::namespace(lexical_parts),
            range,
        )],
        symbols: vec![SymbolFact::new(
            lexical_fqn,
            AnalysisSymbolKind::Method,
            range,
        )],
        ..Default::default()
    };
    let extension_aware = ruby_analysis::core::FileAnalysis {
        methods: vec![MethodFact::new(
            generated_fqn.clone(),
            FullyQualifiedName::namespace(generated_parts.clone()),
            range,
        )],
        symbols: vec![SymbolFact::new(
            generated_fqn,
            AnalysisSymbolKind::Method,
            range,
        )],
        graph_nodes: vec![GraphNodeFact::new(
            FullyQualifiedName::namespace(generated_parts),
            GraphNodeKind::Class,
            range,
        )],
        ..Default::default()
    };

    merge_execution_context_direct_facts(&extension_aware, &mut merged);

    assert_eq!(merged.methods.len(), 1);
    assert!(merged.methods[0].owner.has_generated_owner());
    assert_eq!(merged.symbols.len(), 1);
    assert!(merged.symbols[0].fqn.has_generated_owner());
    assert_eq!(merged.graph_nodes, extension_aware.graph_nodes);
}

#[test]
fn precise_runtime_aware_assignment_replaces_the_same_less_precise_write() {
    let file_id = ruby_analysis::core::SourceFileId(8);
    let subject = TypeSubject::Constant(FullyQualifiedName::try_from("RICH").unwrap());
    let direct_name_range = TextRange::new(file_id, 0, 4);
    let visitor_assignment_range = TextRange::new(file_id, 0, 24);
    let earlier = RubyType::Class(FullyQualifiedName::try_from("RichFixture").unwrap());
    let precise =
        RubyType::Class(FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap());
    let mut merged = vec![TypeFact::new(
        subject.clone(),
        earlier,
        direct_name_range,
        TypeProvenance::Assignment,
    )];

    merge_precise_visitor_type_facts(
        vec![TypeFact::new(
            subject.clone(),
            precise.clone(),
            visitor_assignment_range,
            TypeProvenance::Runtime,
        )],
        &mut merged,
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].subject, subject);
    assert_eq!(merged[0].ruby_type, precise);

    merge_precise_visitor_type_facts(
        vec![TypeFact::new(
            merged[0].subject.clone(),
            RubyType::Unknown,
            visitor_assignment_range,
            TypeProvenance::Runtime,
        )],
        &mut merged,
    );
    assert_eq!(
        merged[0].ruby_type, precise,
        "a later unknown fact must never erase an existing precise type"
    );
}

#[test]
fn later_unknown_visitor_assignment_is_retained_as_a_proof_kill() {
    let file_id = ruby_analysis::core::SourceFileId(9);
    let owner = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Owner").unwrap()],
        ruby_analysis::core::NamespaceKind::Instance,
    );
    let subject = TypeSubject::InstanceVariable {
        owner,
        name: "@value".to_string(),
    };
    let mut merged = vec![TypeFact::new(
        subject.clone(),
        RubyType::Class(FullyQualifiedName::try_from("String").unwrap()),
        TextRange::new(file_id, 4, 10),
        TypeProvenance::Assignment,
    )];

    merge_precise_visitor_type_facts(
        vec![TypeFact::new(
            subject,
            RubyType::Unknown,
            TextRange::new(file_id, 20, 42),
            TypeProvenance::Assignment,
        )],
        &mut merged,
    );

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[1].ruby_type, RubyType::Unknown);
    assert_eq!(merged[1].range, TextRange::new(file_id, 20, 42));
}

#[test]
fn distinct_method_definitions_retain_their_return_facts() {
    let file_id = ruby_analysis::core::SourceFileId(10);
    let method = FullyQualifiedName::method(
        vec![RubyConstant::new("Owner").unwrap()],
        RubyMethod::new("value").unwrap(),
    );
    let subject = TypeSubject::MethodReturn(method);
    let first_range = TextRange::new(file_id, 4, 20);
    let second_range = TextRange::new(file_id, 30, 46);
    let mut merged = vec![TypeFact::new(
        subject.clone(),
        RubyType::string(),
        first_range,
        TypeProvenance::Yard,
    )];

    merge_precise_visitor_type_facts(
        vec![
            TypeFact::new(
                subject.clone(),
                RubyType::integer(),
                first_range,
                TypeProvenance::Inferred,
            ),
            TypeFact::new(
                subject,
                RubyType::integer(),
                second_range,
                TypeProvenance::Yard,
            ),
        ],
        &mut merged,
    );

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].ruby_type, RubyType::string());
    assert_eq!(merged[1].ruby_type, RubyType::integer());
    assert_eq!(merged[1].range, second_range);
}

#[test]
fn precise_inferred_method_return_replaces_same_definition_syntax_seed() {
    let file_id = ruby_analysis::core::SourceFileId(11);
    let method = FullyQualifiedName::method(
        vec![RubyConstant::new("PayloadFactory").unwrap()],
        RubyMethod::new("build").unwrap(),
    );
    let subject = TypeSubject::MethodReturn(method);
    let direct_range = TextRange::new(file_id, 10, 48);
    let visitor_range = TextRange::new(file_id, 12, 45);
    let mut merged = vec![TypeFact::new(
        subject.clone(),
        RubyType::Hash(vec![RubyType::symbol()], vec![RubyType::string()]),
        direct_range,
        TypeProvenance::Inferred,
    )];

    merge_precise_visitor_type_facts(
        vec![TypeFact::new(
            subject.clone(),
            RubyType::string(),
            visitor_range,
            TypeProvenance::Inferred,
        )],
        &mut merged,
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].subject, subject);
    assert_eq!(merged[0].ruby_type, RubyType::string());
    assert_eq!(merged[0].range, visitor_range);
}

#[test]
fn unknown_inferred_method_return_replaces_same_definition_concrete_seed() {
    let file_id = ruby_analysis::core::SourceFileId(12);
    let method = FullyQualifiedName::method(
        vec![RubyConstant::new("Service").unwrap()],
        RubyMethod::new("value").unwrap(),
    );
    let subject = TypeSubject::MethodReturn(method);
    let direct_range = TextRange::new(file_id, 0, 32);
    let visitor_range = TextRange::new(file_id, 2, 30);
    let mut merged = vec![TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        direct_range,
        TypeProvenance::Inferred,
    )];

    merge_precise_visitor_type_facts(
        vec![TypeFact::new(
            subject.clone(),
            RubyType::Unknown,
            visitor_range,
            TypeProvenance::Inferred,
        )],
        &mut merged,
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].subject, subject);
    assert_eq!(merged[0].ruby_type, RubyType::Unknown);
    assert_eq!(merged[0].range, visitor_range);
}

#[test]
fn inferred_method_return_replace_keeps_unrelated_neighbors() {
    let file_id = ruby_analysis::core::SourceFileId(13);
    let method = FullyQualifiedName::method(
        vec![RubyConstant::new("Owner").unwrap()],
        RubyMethod::new("value").unwrap(),
    );
    let method_subject = TypeSubject::MethodReturn(method);
    let first_local = TypeSubject::Local {
        scope_id: 1,
        name: "before".to_string(),
    };
    let last_local = TypeSubject::Local {
        scope_id: 1,
        name: "after".to_string(),
    };
    let seed_range = TextRange::new(file_id, 10, 40);
    let visitor_range = TextRange::new(file_id, 12, 38);
    let mut merged = vec![
        TypeFact::new(
            first_local.clone(),
            RubyType::integer(),
            TextRange::new(file_id, 0, 8),
            TypeProvenance::Assignment,
        ),
        TypeFact::new(
            method_subject.clone(),
            RubyType::integer(),
            seed_range,
            TypeProvenance::Inferred,
        ),
        TypeFact::new(
            last_local.clone(),
            RubyType::string(),
            TextRange::new(file_id, 50, 58),
            TypeProvenance::Assignment,
        ),
    ];

    merge_precise_visitor_type_facts(
        vec![TypeFact::new(
            method_subject.clone(),
            RubyType::string(),
            visitor_range,
            TypeProvenance::Inferred,
        )],
        &mut merged,
    );

    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].subject, first_local);
    assert_eq!(merged[0].ruby_type, RubyType::integer());
    assert_eq!(merged[1].subject, last_local);
    assert_eq!(merged[1].ruby_type, RubyType::string());
    assert_eq!(merged[2].subject, method_subject);
    assert_eq!(merged[2].ruby_type, RubyType::string());
    assert_eq!(merged[2].range, visitor_range);
}

#[test]
fn file_processor_handles_shebang_source_without_crashing() {
    let server = RubyLanguageServer::default();
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let uri = crate::test::harness::fixture_uri("/project/Rakefile");
    let source = "#!/usr/bin/env rake\n# frozen_string_literal: true\nrequire File.expand_path('../config/application', __FILE__)\nExampleApp::Application.load_tasks\n";

    let result = analyze_and_commit(&processor, &server, &uri, source)
        .expect("shebang-bearing Ruby entry points must index successfully");

    assert_eq!(result.semantic_change, SemanticChange::InitialIndex);
}

#[test]
fn reindexing_a_class_declaration_keeps_its_graph_node_and_mixin_lookup() {
    let server = RubyLanguageServer::default();
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let helpers_uri = crate::test::harness::fixture_uri("/project/helpers.rb");
    let app_uri = crate::test::harness::fixture_uri("/project/app.rb");
    let helpers = "module API\n  module Catalog\n    def get_images\n    end\n  end\n\n  include Catalog\nend\n";
    let app = "class Base\n  include API\nend\n\nclass PlatformApp < Base\n  def route\n    get_images\n  end\nend\n";

    analyze_and_commit(&processor, &server, &helpers_uri, helpers).unwrap();
    analyze_and_commit(&processor, &server, &app_uri, app).unwrap();
    // Second pass mirrors didOpen-then-cold-index: the class constant already
    // carries a ClassReference from the first declaration of this same file.
    analyze_and_commit(&processor, &server, &app_uri, app).unwrap();

    let platform_app =
        FullyQualifiedName::namespace(vec![RubyConstant::new("PlatformApp").unwrap()]);
    let method = RubyMethod::new("get_images").unwrap();
    let engine = server.orphan_engine().read();
    let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    assert!(
        query.namespace_exists(&platform_app),
        "reindexing class PlatformApp must keep its graph node so mixin lookup remains possible"
    );
    let request = ruby_analysis::engine::lookup::MethodRequest::new(
        ruby_analysis::engine::lookup::LookupReceiver::Namespace(&platform_app),
        method,
        ruby_analysis::engine::lookup::MethodWant::Callees,
    );
    let callees = ruby_analysis::engine::lookup::method(&query, request)
        .into_callees()
        .expect("PlatformApp must remain a resolvable method owner after reindex");
    assert!(
        callees.iter().any(|callee| {
            callee.owner.to_string().contains("Catalog")
                && !callee.definition_ranges.is_empty()
        }),
        "helper method must remain reachable through Base/API includes after reindex, got {callees:?}"
    );
}

#[test]
fn file_processor_reopens_a_cross_file_class_alias_under_the_original_owner() {
    let server = RubyLanguageServer::default();
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let declaration_uri = crate::test::harness::fixture_uri("/project/types.rb");
    let reopening_uri = crate::test::harness::fixture_uri("/project/reopening.rb");

    analyze_and_commit(
        &processor,
        &server,
        &declaration_uri,
        "module Types\n  class Original\n  end\n  Alias = Original\nend\n",
    )
    .unwrap();
    analyze_and_commit(
        &processor,
        &server,
        &reopening_uri,
        "module Types\n  class Alias\n    def from_other_file\n    end\n  end\nend\n",
    )
    .unwrap();

    let expected = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Types").unwrap(),
            RubyConstant::new("Original").unwrap(),
        ],
        RubyMethod::new("from_other_file").unwrap(),
    );
    let shadow = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Types").unwrap(),
            RubyConstant::new("Alias").unwrap(),
        ],
        RubyMethod::new("from_other_file").unwrap(),
    );
    let engine = server.orphan_engine().read();
    assert_eq!(engine.method_facts_for(&expected).len(), 1);
    assert!(engine.method_facts_for(&shadow).is_empty());
}

#[test]
fn explicit_project_engine_owns_external_gem_source() {
    let server = RubyLanguageServer::default();
    let project_uri = crate::test::harness::fixture_uri("/workspace/server/");
    let project = server.add_workspace(project_uri);
    let dependency_uri =
        crate::test::harness::fixture_uri("/workspace/server/vendor/cache/pbkdf2/lib/pbkdf2.rb");
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());

    processor
        .collect_file_facts_as_deferred_resolution_in_engine(
            &dependency_uri,
            "class PBKDF2\nend\n",
            project.analysis_engine.clone(),
            SourceKind::Gem,
        )
        .unwrap();

    let engine = project.analysis_engine.read();
    let path = dependency_uri.to_file_path().unwrap();
    let file_id = engine
        .view()
        .file_id(&path)
        .expect("gem source must be registered");
    assert_eq!(engine.view().file(file_id).unwrap().kind, SourceKind::Gem);
    assert!(server
        .orphan_engine()
        .read()
        .view()
        .file_id(&path)
        .is_none());
}

fn collect_gem_template_facts(source: &str) -> FileAnalysis {
    let server = RubyLanguageServer::default();
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let producer_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    let dependency_uri = crate::test::harness::fixture_uri("/shared/gems/widget/lib/widget.rb");
    let template = processor
        .collect_project_neutral_file_template_as_deferred_resolution_in_engine(
            &dependency_uri,
            source,
            producer_engine,
            SourceKind::Gem,
            Arc::new(HashSet::new()),
        )
        .unwrap();
    template.instantiate(ruby_analysis::core::SourceFileId(1))
}

fn gem_method_returns(source: &str) -> Vec<(RubyType, TypeProvenance)> {
    collect_gem_template_facts(source)
        .types
        .into_iter()
        .filter_map(|fact| match fact.subject {
            TypeSubject::MethodReturn(_) => Some((fact.ruby_type, fact.provenance)),
            _ => None,
        })
        .collect()
}

#[test]
fn gem_collection_does_not_persist_inferred_method_returns() {
    let returns = gem_method_returns("class SharedWidget\n  def value\n    1\n  end\nend\n");
    assert!(
        returns.iter().all(
            |(ruby_type, provenance)| *provenance != TypeProvenance::Inferred
                || *ruby_type == RubyType::Unknown
        ),
        "dependency collection must not persist inferred-only method returns: {returns:?}"
    );
    assert!(
        !returns
            .iter()
            .any(|(ruby_type, _)| *ruby_type == RubyType::integer()),
        "inferred Integer must not survive gem product templates: {returns:?}"
    );
}

#[test]
fn gem_collection_persists_yard_method_returns() {
    let returns = gem_method_returns(
        "class SharedWidget\n  # @return [Integer]\n  def value\n    'cached'\n  end\nend\n",
    );
    assert!(
        returns.iter().any(|(ruby_type, provenance)| {
            *ruby_type == RubyType::integer() && *provenance == TypeProvenance::Yard
        }),
        "YARD-declared gem returns must remain in project-neutral templates: {returns:?}"
    );
}

#[test]
fn external_gem_collection_can_emit_a_rebindable_project_neutral_template() {
    let server = RubyLanguageServer::default();
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let producer_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    let dependency_uri = crate::test::harness::fixture_uri("/shared/gems/widget/lib/widget.rb");
    let source = "class SharedWidget\n  def value\n    'cached'\n  end\nend\n";

    let template = processor
        .collect_project_neutral_file_template_as_deferred_resolution_in_engine(
            &dependency_uri,
            source,
            producer_engine,
            SourceKind::Gem,
            Arc::new(HashSet::new()),
        )
        .unwrap();

    let mut consumer = AnalysisEngine::new();
    consumer.register_file(ruby_analysis::engine::SourceFileInput {
        path: crate::test::harness::fixture_path("/consumer/project.rb"),
        content: String::new(),
        kind: SourceKind::Project,
    });
    let dependency_file = consumer.register_file(ruby_analysis::engine::SourceFileInput {
        path: crate::test::harness::fixture_path("/consumer/cache/widget/lib/widget.rb"),
        content: source.to_string(),
        kind: SourceKind::Gem,
    });
    consumer.replace_facts(
        dependency_file,
        template.instantiate(dependency_file),
        ResolveMode::Immediate,
    );

    let definitions = AnalysisQuery::new(&consumer)
        .constant_definition_ranges(&[RubyConstant::new("SharedWidget").unwrap()], &[]);
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].file_id, dependency_file);
    assert_eq!(
        consumer.view().file(definitions[0].file_id).unwrap().path,
        crate::test::harness::fixture_path("/consumer/cache/widget/lib/widget.rb")
    );
}
