use super::collection::map_owned_project_inputs;
use super::navigation::{prioritize_project_files, select_navigation_demand_files};
use super::*;
use crate::environment::config::IndexingConfig;
use crate::environment::runtime::jruby::imports::JrubyImportProvider;
use crate::environment::runtime::jruby::java_catalog::{JavaClassDeclaration, ProjectJavaCatalog};
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::AnalysisQuery;
use ruby_fast_lsp_jvm_metadata::ClassFile;
use std::collections::BTreeMap;
use tempfile::TempDir;
use tower_lsp::lsp_types::{
    DidOpenTextDocumentParams, InlayHintParams, Position, Range, TextDocumentIdentifier,
    TextDocumentItem, Url,
};

#[test]
fn project_input_partitions_consume_non_clone_values_in_order() {
    struct OwnedOnly(u8);

    let values = vec![OwnedOnly(1), OwnedOnly(2), OwnedOnly(3), OwnedOnly(4)];
    let consumed = map_owned_project_inputs(values, 2, &|OwnedOnly(value)| value);

    assert_eq!(
        consumed,
        vec![1, 2, 3, 4],
        "priority and exhaustive partitions must preserve deterministic input order while \
             transferring each non-Clone source owner exactly once"
    );
}

#[test]
fn active_constant_keys_prioritize_matching_project_file_stems() {
    let files = vec![
        PathBuf::from("/project/a.rb"),
        PathBuf::from("/project/account.rb"),
        PathBuf::from("/project/account_record.rb"),
        PathBuf::from("/project/z.rb"),
    ];
    let priority_keys = HashSet::from(["accountrecord".to_string()]);

    let (files, priority_count) = prioritize_project_files(files, &priority_keys);

    assert_eq!(priority_count, 2);
    assert_eq!(
        files,
        vec![
            PathBuf::from("/project/account.rb"),
            PathBuf::from("/project/account_record.rb"),
            PathBuf::from("/project/a.rb"),
            PathBuf::from("/project/z.rb"),
        ],
        "active-document constant targets must move exact and conventional base filenames \
            first while preserving the exhaustive order of every nonmatching project file"
    );
}

#[test]
fn exact_navigation_demand_promotes_a_late_project_file_before_the_next_batch() {
    let mut pending_files = (0..20)
        .map(|index| PathBuf::from(format!("/project/ordinary_{index:02}.rb")))
        .chain([PathBuf::from("/project/account_record.rb")])
        .collect::<Vec<_>>();

    let selection = select_navigation_demand_files(
        &mut pending_files,
        &HashSet::new(),
        &["accountrecord".to_string()],
    );

    assert_eq!(
        selection.files,
        vec![PathBuf::from("/project/account_record.rb")],
        "a bounded exact request must remove its conventional definition candidate from the \
             exhaustive tail before unrelated files are selected"
    );
    assert_eq!(selection.completed_keys, vec!["accountrecord".to_string()]);
    assert!(selection.deferred_keys.is_empty());
    assert!(
        pending_files
            .iter()
            .all(|path| path != Path::new("/project/account_record.rb")),
        "the demanded file must not be parsed again by exhaustive collection"
    );
}

#[test]
fn navigation_demand_without_a_conventional_file_candidate_waits_for_project_completion() {
    let mut pending_files = vec![PathBuf::from("/project/legacy_location.rb")];

    let selection = select_navigation_demand_files(
        &mut pending_files,
        &HashSet::new(),
        &["unconventionalclass".to_string()],
    );

    assert!(selection.files.is_empty());
    assert!(selection.completed_keys.is_empty());
    assert_eq!(
        selection.deferred_keys,
        vec!["unconventionalclass".to_string()],
        "a filename heuristic cannot claim that the semantic target was processed when no \
             bounded candidate exists"
    );
}

#[test]
fn exhaustive_project_tail_is_yielded_in_bounded_deterministic_batches() {
    let mut indexer = IndexerProject::new(
        PathBuf::from("/project"),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.pending_project_files = Some(
        (0..10)
            .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
            .collect(),
    );

    assert_eq!(
        indexer.take_next_remaining_project_files(4),
        (0..4)
            .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
            .collect::<Vec<_>>()
    );
    assert_eq!(indexer.remaining_project_file_count(), 6);
    assert_eq!(
        indexer.take_next_remaining_project_files(4),
        (4..8)
            .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        indexer.take_next_remaining_project_files(4),
        (8..10)
            .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
            .collect::<Vec<_>>()
    );
    assert_eq!(indexer.remaining_project_file_count(), 0);
}

#[test]
fn delayed_project_discovery_cannot_borrow_a_replacement_generation() {
    let fixture = TempDir::new().unwrap();
    let server = RubyLanguageServer::default();
    let workspace = server.add_workspace(Url::from_directory_path(fixture.path()).unwrap());
    let old_run = workspace.begin_indexing_run();
    let mut indexer = IndexerProject::new(
        fixture.path().to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_progress_generation(Some(old_run.generation()));
    let new_run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(
            new_run.generation(),
            crate::indexer::scheduling::status::IndexingPhase::IndexingProject,
            None,
            None,
        )
        .unwrap();
    indexer.begin_project_file_progress(3, &server);
    let snapshot = workspace.indexing_status.snapshot();
    assert_eq!(snapshot.generation, new_run.generation());
    assert_eq!(
        snapshot.completed, None,
        "old work must retain its own generation even when discovery finishes after a restart"
    );
    assert_eq!(snapshot.total, None);
}

#[test]
fn project_indexing_status_reports_completed_files_against_a_stable_total() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    for name in ["alpha.rb", "beta.rb", "gamma.rb", "delta.rb"] {
        std::fs::write(root.join(name), "class Sample\nend\n").unwrap();
    }

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let run = workspace_state.begin_indexing_run();
    workspace_state
        .indexing_status
        .transition(
            run.generation(),
            crate::indexer::scheduling::status::IndexingPhase::IndexingProject,
            None,
            None,
        )
        .unwrap();

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_progress_generation(Some(run.generation()));
    indexer.set_navigation_priority_keys(HashSet::from(["alpha".to_string()]), HashSet::new());

    indexer
        .collect_initial_project_navigation_demand_facts(&[], &server)
        .unwrap();
    let discovered = workspace_state.indexing_status.snapshot();
    assert_eq!(discovered.total, Some(4));
    assert_eq!(
        discovered.completed,
        Some(0),
        "file discovery must publish a stable denominator before the first batch"
    );

    indexer.finish_project_navigation_facts(&server).unwrap();
    let after_frontier = workspace_state.indexing_status.snapshot();
    assert_eq!(after_frontier.total, Some(4));
    assert_eq!(after_frontier.completed, Some(1));

    indexer.collect_remaining_project_facts(&server).unwrap();
    let after_all = workspace_state.indexing_status.snapshot();
    assert_eq!(after_all.total, Some(4));
    assert_eq!(after_all.completed, Some(4));
    let reported_completed = server
        .indexing_progress_reports()
        .into_iter()
        .map(|(_, completed, _)| completed)
        .collect::<Vec<_>>();
    assert!(
        reported_completed.contains(&2) && reported_completed.contains(&3),
        "a multi-file batch must report each collected file, not only the batch boundary; got {reported_completed:?}"
    );
}

fn jruby_provider(class_names: &[&str]) -> JrubyImportProvider {
    jruby_provider_with_superclasses(
        &class_names
            .iter()
            .map(|name| (*name, "java/lang/Object"))
            .collect::<Vec<_>>(),
    )
}

fn jruby_provider_with_superclasses(
    classes_with_superclasses: &[(&str, &str)],
) -> JrubyImportProvider {
    let classes = classes_with_superclasses
        .iter()
        .map(|(name, superclass)| {
            (
                (*name).to_string(),
                JavaClassDeclaration {
                    class: Arc::new(ClassFile {
                        minor_version: 0,
                        major_version: 61,
                        access_flags: 0x0021,
                        name: (*name).to_string(),
                        super_name: Some((*superclass).to_string()),
                        interfaces: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                        source_file: None,
                        signature: None,
                        annotations: Vec::new(),
                        inner_classes: Vec::new(),
                        record_components: Vec::new(),
                        module_name: None,
                    }),
                    artifact_path: PathBuf::from("/fixture/runtime.jar"),
                    artifact_fingerprint_sha256: "fixture".to_string(),
                    entry_name: format!("{name}.class"),
                    release: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    JrubyImportProvider::new(Arc::new(ProjectJavaCatalog {
        classpath_fingerprint_sha256: "fixture-classpath".to_string(),
        classes,
        duplicates: Vec::new(),
    }))
}

#[test]
fn configured_project_files_drive_dependency_scanning() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    std::fs::write(root.join("app.rb"), "gem 'rack'\n").unwrap();
    std::fs::write(root.join("bin/console"), "gem 'rails'\n").unwrap();
    std::fs::write(root.join("vendor/generated.rb"), "gem 'debug'\n").unwrap();

    let indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig {
            included_patterns: vec!["bin/*".to_string()],
            excluded_patterns: vec!["vendor/**/*".to_string()],
            ..IndexingConfig::default()
        },
    );

    indexer.scan_for_dependencies().unwrap();

    assert!(indexer.requires_gem("rack"));
    assert!(indexer.requires_gem("rails"));
    assert!(!indexer.requires_gem("debug"));
}

#[tokio::test]
async fn project_stage_resolves_open_documents_and_defers_closed_candidates() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let open_path = root.join("open.rb");
    let closed_path = root.join("closed.rb");
    let definition_path = root.join("user.rb");
    std::fs::write(&open_path, "User.new\n").unwrap();
    std::fs::write(&closed_path, "User.new\n").unwrap();
    std::fs::write(&definition_path, "class User\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace = server.add_workspace(Url::from_directory_path(root).unwrap());
    let open_uri = Url::from_file_path(&open_path).unwrap();
    crate::lsp::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: open_uri,
                language_id: "ruby".to_string(),
                version: 1,
                text: "User.new\n".to_string(),
            },
        },
    )
    .await;

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.collect_project_facts(&server).unwrap();

    let engine = workspace.analysis_engine.read();
    let open_file = engine.file_id(&open_path).unwrap();
    let closed_file = engine.file_id(&closed_path).unwrap();
    let query = AnalysisQuery::new(&engine);
    assert!(
        !query.references_in_file(open_file).is_empty(),
        "the open document must have its project references resolved"
    );
    assert!(
        query.references_in_file(closed_file).is_empty(),
        "closed-file candidates must remain deferred during project-navigation staging"
    );
    drop(engine);

    workspace.analysis_engine.write().resolve();
    assert!(
        !AnalysisQuery::new(&workspace.analysis_engine.read())
            .references_in_file(closed_file)
            .is_empty(),
        "the final complete resolution must materialize the deferred closed-file candidate"
    );
}

#[tokio::test]
async fn cold_project_collection_cannot_overwrite_newer_open_document_facts() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let utility_path = root.join("utility.rb");
    let caller_path = root.join("caller.rb");
    let stale_disk_source = "module Example\n  module Utility\n  end\nend\n";
    let open_source = "module Example\n  module Utility\n    def self.lookup(value)\n      value\n    end\n  end\nend\n";
    let caller_source = "Example::Utility.lookup(\"value\")\n";
    std::fs::write(&utility_path, stale_disk_source).unwrap();
    std::fs::write(&caller_path, caller_source).unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    for (path, text) in [(&utility_path, open_source), (&caller_path, caller_source)] {
        crate::lsp::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: Url::from_file_path(path).unwrap(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: text.to_string(),
                },
            },
        )
        .await;
    }

    let caller_file = workspace_state
        .analysis_engine
        .read()
        .file_id(&caller_path)
        .unwrap();
    assert!(
        !AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .resolved_reference_definition_ranges_at(caller_file, 19)
            .is_empty(),
        "the open-document pass must initially resolve the singleton method"
    );

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.collect_project_facts(&server).unwrap();
    workspace_state.analysis_engine.write().resolve();

    let engine = workspace_state.analysis_engine.read();
    let utility_file = engine.file_id(&utility_path).unwrap();
    assert!(
        engine.file_content_matches(utility_file, open_source),
        "cold indexing must retain the editor's newer source snapshot"
    );
    assert!(
        !AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(caller_file, 19)
            .is_empty(),
        "a stale cold-index batch must not erase method facts from a newer open document"
    );
}

#[test]
fn cold_project_result_is_independent_of_a_prior_identical_file_pass() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let path = root.join("user.rb");
    let source = "class User\n  def normalized_name\n    name.upcase\n  end\n\n  def name\n    \"Ada\"\n  end\nend\n\nUser.new.normalized_name\n";
    std::fs::write(&path, source).unwrap();

    let collect = |preindex: bool| {
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        if preindex {
            FileProcessor::new()
                .process_file_current_file_resolution_forced(
                    &Url::from_file_path(&path).unwrap(),
                    source,
                    &server,
                )
                .unwrap();
        }
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.collect_project_facts(&server).unwrap();
        workspace_state.analysis_engine.write().resolve();
        let fingerprint = workspace_state
            .analysis_engine
            .read()
            .semantic_result_fingerprint();
        fingerprint
    };

    assert_eq!(
        collect(false),
        collect(true),
        "byte-identical project collection must not consume stale facts from an earlier pass of the same file"
    );
}

#[test]
fn project_navigation_frontier_releases_before_exhaustive_source_collection() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("user.rb");
    let background_path = root.join("report.rb");
    std::fs::write(&active_path, "class User\nend\n").unwrap();
    std::fs::write(&background_path, "class Report\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["user".to_string()]), HashSet::new());

    indexer.collect_project_navigation_facts(&server).unwrap();

    let user = ruby_analysis::core::RubyConstant::new("User").unwrap();
    let report = ruby_analysis::core::RubyConstant::new("Report").unwrap();
    {
        let engine = workspace_state.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        assert!(
            !query
                .constant_definition_ranges(&[user.clone()], &[])
                .is_empty(),
            "the exact active target must be queryable after the navigation frontier"
        );
        assert!(
            query
                .constant_definition_ranges(&[report.clone()], &[])
                .is_empty(),
            "unrelated project source must remain pending until the exhaustive stage"
        );
    }

    indexer.collect_remaining_project_facts(&server).unwrap();

    let engine = workspace_state.analysis_engine.read();
    assert!(
        !AnalysisQuery::new(&engine)
            .constant_definition_ranges(&[report], &[])
            .is_empty(),
        "the exhaustive stage must complete the same isolated project engine"
    );
}

#[test]
fn queued_exact_demand_is_queryable_before_unrelated_active_candidates() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::write(root.join("account_record.rb"), "class AccountRecord\nend\n").unwrap();
    std::fs::write(root.join("report.rb"), "class Report\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["report".to_string()]), HashSet::new());

    let selection = indexer
        .collect_initial_project_navigation_demand_facts(&["accountrecord".to_string()], &server)
        .unwrap();

    assert_eq!(selection.completed_keys, vec!["accountrecord".to_string()]);
    assert!(selection.deferred_keys.is_empty());
    let user = ruby_analysis::core::RubyConstant::new("AccountRecord").unwrap();
    let report = ruby_analysis::core::RubyConstant::new("Report").unwrap();
    {
        let engine = workspace_state.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        assert!(
            !query
                .constant_definition_ranges(&[user.clone()], &[])
                .is_empty(),
            "the queued exact target must be queryable before unrelated active candidates"
        );
        assert!(
            query
                .constant_definition_ranges(&[report.clone()], &[])
                .is_empty(),
            "an unrelated active candidate must remain pending when the exact demand wakes"
        );
    }

    indexer.finish_project_navigation_facts(&server).unwrap();
    let engine = workspace_state.analysis_engine.read();
    assert!(
        !AnalysisQuery::new(&engine)
            .constant_definition_ranges(&[report], &[])
            .is_empty(),
        "the rest of the active frontier must remain semantically complete"
    );
}

#[test]
fn navigation_demand_completes_when_the_frontier_already_processed_its_file() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::write(root.join("account.rb"), "class AccountRecord\nend\n").unwrap();
    std::fs::write(root.join("report.rb"), "class Report\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .set_navigation_priority_keys(HashSet::from(["accountrecord".to_string()]), HashSet::new());

    indexer.collect_project_navigation_facts(&server).unwrap();
    let selection = indexer.take_navigation_demand_files(&["accountrecord".to_string()]);

    assert!(
        selection.files.is_empty(),
        "an already indexed frontier file must never be parsed a second time"
    );
    assert_eq!(selection.completed_keys, vec!["accountrecord".to_string()]);
    assert!(
        selection.deferred_keys.is_empty(),
        "a request whose matching frontier file is queryable must wake immediately"
    );
}

#[test]
fn exhaustive_batches_share_one_immutable_pre_collection_namespace_context() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::write(root.join("seed.rb"), "class Seed\nend\n").unwrap();
    let parent_path = root.join("a_parent.rb");
    let child_path = root.join("b_child.rb");
    std::fs::write(&parent_path, "class Parent\nend\n").unwrap();
    std::fs::write(&child_path, "class Child < Parent\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["seed".to_string()]), HashSet::new());
    indexer.collect_project_navigation_facts(&server).unwrap();

    let parent_batch = indexer.take_next_remaining_project_files(1);
    assert_eq!(parent_batch, vec![parent_path]);
    indexer
        .collect_project_file_batch(&parent_batch, &server, false)
        .unwrap();
    let child_batch = indexer.take_next_remaining_project_files(1);
    assert_eq!(child_batch, vec![child_path.clone()]);
    indexer
        .collect_project_file_batch(&child_batch, &server, false)
        .unwrap();

    let child = ruby_analysis::core::FullyQualifiedName::namespace(vec![
        ruby_analysis::core::RubyConstant::new("Child").unwrap(),
    ]);
    {
        let engine = workspace_state.analysis_engine.read();
        assert!(
            engine.unresolved_graph_edges().iter().any(|edge| {
                edge.source == child && edge.kind == ruby_analysis::core::GraphEdgeKind::Superclass
            }),
            "a later batch must not observe namespaces introduced by an arbitrary earlier \
                 exhaustive batch"
        );
    }

    workspace_state.analysis_engine.write().resolve();
    let engine = workspace_state.analysis_engine.read();
    assert!(
        engine.unresolved_graph_edges().iter().all(|edge| {
            edge.source != child || edge.kind != ruby_analysis::core::GraphEdgeKind::Superclass
        }),
        "the coordinator's final semantic resolution must resolve the deferred superclass"
    );
}

#[tokio::test]
async fn project_solves_transitive_cross_file_value_constants_after_early_did_open() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let first_consumer_path = root.join("first_consumer.rb");
    let second_consumer_path = root.join("second_consumer.rb");
    let alias_path = root.join("alias.rb");
    let base_path = root.join("base.rb");
    let cycle_consumer_path = root.join("cycle_consumer.rb");
    let first_consumer = r#"module Marketplace::Platform
  module Commerce
    class OrderProcessor
      def self.first_code(retryable)
        if retryable
          code = Marketplace::Platform::Errors::PaymentCodes::RETRY
        else
          code = Marketplace::Platform::Errors::PaymentCodes::FAILED
        end
        code
      end

      def self.fallback_code
        fallback = nil
        fallback ||= Marketplace::Platform::Errors::PaymentCodes::FAILED
        fallback
      end
    end
  end
end
"#;
    let second_consumer = r#"module Marketplace::Platform
  module Commerce
    class OrderProcessor
      def self.second_code
        result = Marketplace::Platform::Errors::PaymentCodes::RETRY
        result
      end
    end
  end
end
"#;
    std::fs::write(&first_consumer_path, first_consumer).unwrap();
    std::fs::write(&second_consumer_path, second_consumer).unwrap();
    std::fs::write(
        &alias_path,
        r#"module Marketplace
  module Platform
    module Commerce
    end

    module Errors
      class PaymentCodes
        RETRY = BaseCodes::RETRY
        FAILED = BaseCodes::FAILED
      end
    end
  end
end
"#,
    )
    .unwrap();
    std::fs::write(
        &base_path,
        r#"module BaseCodes
  RETRY = "retry".freeze
  FAILED = "failed".freeze
end
"#,
    )
    .unwrap();
    std::fs::write(root.join("cycle_a.rb"), "CycleA = CycleB\n").unwrap();
    std::fs::write(root.join("cycle_b.rb"), "CycleB = CycleA\n").unwrap();
    let cycle_consumer = "cycle = CycleA\n";
    std::fs::write(&cycle_consumer_path, cycle_consumer).unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    crate::lsp::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Url::from_file_path(&first_consumer_path).unwrap(),
                language_id: "ruby".to_string(),
                version: 1,
                text: first_consumer.to_string(),
            },
        },
    )
    .await;
    {
        let engine = workspace_state.analysis_engine.read();
        let file_id = engine.file_id(&first_consumer_path).unwrap();
        let assignment_start = u32::try_from(first_consumer.find("code =").unwrap()).unwrap();
        assert_eq!(
            AnalysisQuery::new(&engine).variable_assignment_type_at(
                ruby_analysis::engine::VariableTypeKind::Local,
                "code",
                file_id,
                assignment_start,
                assignment_start + u32::try_from("code".len()).unwrap(),
            ),
            Some(ruby_analysis::core::RubyType::Unknown),
            "an unresolved value constant must remain Unknown; Class<...> is only sound after a class declaration is proven"
        );
    }
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .set_navigation_priority_keys(HashSet::from(["firstconsumer".to_string()]), HashSet::new());
    indexer.collect_project_navigation_facts(&server).unwrap();
    indexer.collect_remaining_project_facts(&server).unwrap();
    workspace_state.analysis_engine.write().resolve();

    let engine = workspace_state.analysis_engine.read();
    let query = AnalysisQuery::new(&engine);
    for parts in [
        vec!["BaseCodes", "RETRY"],
        vec!["Marketplace", "Platform", "Errors", "PaymentCodes", "RETRY"],
    ] {
        let constant = FullyQualifiedName::constant(
            parts
                .into_iter()
                .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            query.constant_value_type(&constant),
            Some(ruby_analysis::core::RubyType::string()),
            "transitive constant equations must solve each alias before consumers"
        );
    }
    let first_file_id = engine.file_id(&first_consumer_path).unwrap();
    let final_read_offset =
        u32::try_from(first_consumer.rfind("        code\n").unwrap() + 8).unwrap();
    assert_eq!(
        query.local_read_type_at(first_file_id, final_read_offset),
        Some(ruby_analysis::core::RubyType::string()),
        "flow reads after a branch must consume the joined constant equation"
    );
    for (path, source, name) in [
        (&first_consumer_path, first_consumer, "code"),
        (&second_consumer_path, second_consumer, "result"),
    ] {
        let file_id = engine.file_id(path).unwrap();
        let assignment_start = u32::try_from(source.find(&format!("{name} =")).unwrap()).unwrap();
        let assignment_end = assignment_start + u32::try_from(name.len()).unwrap();
        assert_eq!(
            AnalysisQuery::new(&engine).variable_assignment_type_at(
                ruby_analysis::engine::VariableTypeKind::Local,
                name,
                file_id,
                assignment_start,
                assignment_end,
            ),
            Some(ruby_analysis::core::RubyType::string()),
            "one equation solve must update every consumer of a transitive value-constant alias"
        );
    }
    for (path, source, method) in [
        (&first_consumer_path, first_consumer, "first_code"),
        (&second_consumer_path, second_consumer, "second_code"),
    ] {
        let file_id = engine.file_id(path).unwrap();
        let method_offset = u32::try_from(source.find(method).unwrap()).unwrap();
        assert_eq!(
            query.method_return_type_at(method, file_id, method_offset),
            Some(ruby_analysis::core::RubyType::string()),
            "method-return equations must retain the same constant dependency as assignment facts"
        );
    }
    let fallback_start = u32::try_from(first_consumer.find("fallback ||=").unwrap()).unwrap();
    assert_eq!(
        query.variable_assignment_type_at(
            ruby_analysis::engine::VariableTypeKind::Local,
            "fallback",
            first_file_id,
            fallback_start,
            fallback_start + u32::try_from("fallback".len()).unwrap(),
        ),
        Some(ruby_analysis::core::RubyType::string()),
        "a late-resolved value constant must update the stable source assignment for ||= writes"
    );
    let cycle_file_id = engine.file_id(&cycle_consumer_path).unwrap();
    assert_eq!(
        query.variable_assignment_type_at(
            ruby_analysis::engine::VariableTypeKind::Local,
            "cycle",
            cycle_file_id,
            0,
            u32::try_from("cycle".len()).unwrap(),
        ),
        Some(ruby_analysis::core::RubyType::Unknown),
        "a base-free constant cycle must stay Unknown instead of becoming a class-object guess"
    );
    drop(engine);

    let hints = crate::lsp::capabilities::presentation::inlay_hints::handle_inlay_hints(
        &server,
        InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(&first_consumer_path).unwrap(),
            },
            range: Range::new(Position::new(0, 0), Position::new(100, 0)),
        },
    )
    .await;
    let assignment_lines = first_consumer
        .lines()
        .enumerate()
        .filter_map(|(line, text)| text.contains("code =").then_some(line as u32))
        .collect::<Vec<_>>();
    for line in assignment_lines {
        assert!(
            hints.iter().any(|hint| {
                hint.position.line == line
                    && crate::test::harness::get_hint_label(hint) == ": String"
            }),
            "the open document's inlay hints must prefer solved engine facts over its pre-index variable-scope snapshot; hints={hints:?}"
        );
    }
}

#[tokio::test]
async fn project_resolves_early_nested_constructor_to_later_top_level_class() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let consumer_path = root.join("consumer.rb");
    let definition_path = root.join("registry.rb");
    let consumer = r#"module Portal
  module Accounts
    class Controller
      def build
        tags = Registry.new
        tags
      end
    end
  end
end
"#;
    std::fs::write(&consumer_path, consumer).unwrap();
    std::fs::write(&definition_path, "class Registry\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    crate::lsp::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Url::from_file_path(&consumer_path).unwrap(),
                language_id: "ruby".to_string(),
                version: 1,
                text: consumer.to_string(),
            },
        },
    )
    .await;

    let assignment_start = u32::try_from(consumer.find("tags =").unwrap()).unwrap();
    let assignment_end = assignment_start + u32::try_from("tags".len()).unwrap();
    {
        let engine = workspace_state.analysis_engine.read();
        let file_id = engine.file_id(&consumer_path).unwrap();
        assert_eq!(
            AnalysisQuery::new(&engine).variable_assignment_type_at(
                ruby_analysis::engine::VariableTypeKind::Local,
                "tags",
                file_id,
                assignment_start,
                assignment_end,
            ),
            Some(ruby_analysis::core::RubyType::Unknown),
            "an unresolved constructor must remain Unknown instead of fabricating a class in the current lexical namespace"
        );
    }

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["consumer".to_string()]), HashSet::new());
    indexer.collect_project_navigation_facts(&server).unwrap();
    indexer.collect_remaining_project_facts(&server).unwrap();
    workspace_state.analysis_engine.write().resolve();

    let engine = workspace_state.analysis_engine.read();
    let file_id = engine.file_id(&consumer_path).unwrap();
    let expected = ruby_analysis::core::RubyType::Class(FullyQualifiedName::constant(vec![
        ruby_analysis::core::RubyConstant::new("Registry").unwrap(),
    ]));
    assert_eq!(
        AnalysisQuery::new(&engine).variable_assignment_type_at(
            ruby_analysis::engine::VariableTypeKind::Local,
            "tags",
            file_id,
            assignment_start,
            assignment_end,
        ),
        Some(expected),
        "final semantic resolution must bind the constructor to the proven top-level class"
    );
    let method_offset = u32::try_from(consumer.find("build").unwrap()).unwrap();
    assert_eq!(
        AnalysisQuery::new(&engine).method_return_type_at("build", file_id, method_offset),
        Some(ruby_analysis::core::RubyType::Class(
            FullyQualifiedName::constant(vec![
                ruby_analysis::core::RubyConstant::new("Registry").unwrap(),
            ]),
        )),
        "flow and method-return equations must consume the same resolved constructor identity"
    );
    drop(engine);

    let hints = crate::lsp::capabilities::presentation::inlay_hints::handle_inlay_hints(
        &server,
        InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(&consumer_path).unwrap(),
            },
            range: Range::new(Position::new(0, 0), Position::new(20, 0)),
        },
    )
    .await;
    assert!(
        hints.iter().any(|hint| {
            hint.position.line == 4 && crate::test::harness::get_hint_label(hint) == ": Registry"
        }),
        "the open consumer must display the resolved top-level constructor type; hints={hints:?}"
    );
}

#[test]
fn exhaustive_semantics_do_not_depend_on_batch_boundaries() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::write(root.join("seed.rb"), "class Seed\nend\n").unwrap();
    std::fs::write(
        root.join("a_service.rb"),
        "class Target\n  def name\n    String\n  end\nend\n\
             class Service\n  def target\n    Target.new\n  end\n\
             delegate :name, to: :target\nend\n",
    )
    .unwrap();
    std::fs::write(
        root.join("b_consumer.rb"),
        "class Consumer\n  def value\n    Service.new.name\n  end\nend\n",
    )
    .unwrap();

    let mut expected = None;
    for batch_size in [1, 2] {
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.set_navigation_priority_keys(HashSet::from(["seed".to_string()]), HashSet::new());
        indexer.collect_project_navigation_facts(&server).unwrap();

        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| {
                while indexer.remaining_project_file_count() > 0 {
                    let batch = indexer.take_next_remaining_project_files(batch_size);
                    indexer
                        .collect_project_file_batch(&batch, &server, false)
                        .unwrap();
                }
            });
        indexer.finish_remaining_project_facts();
        workspace_state.analysis_engine.write().resolve();
        let actual = workspace_state
            .analysis_engine
            .read()
            .semantic_result_fingerprint();

        if let Some(expected) = expected {
            assert_eq!(
                actual, expected,
                "exhaustive project facts must not depend on arbitrary coordinator batch \
                     boundaries"
            );
        } else {
            expected = Some(actual);
        }
    }
}

#[test]
fn parallel_batch_collection_has_a_stable_semantic_result() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::write(root.join("seed.rb"), "class Seed\nend\n").unwrap();
    for index in 0..32 {
        std::fs::write(
            root.join(format!("model_{index:02}.rb")),
            format!(
                "class Model{index:02}\n  def sibling\n    Model{:02}.new\n  end\nend\n",
                (index + 1) % 32
            ),
        )
        .unwrap();
    }

    let mut expected = None;
    for _ in 0..4 {
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.set_navigation_priority_keys(HashSet::from(["seed".to_string()]), HashSet::new());
        indexer.collect_project_navigation_facts(&server).unwrap();

        let batch = indexer.take_next_remaining_project_files(64);
        assert_eq!(batch.len(), 32);
        indexer
            .collect_project_file_batch(&batch, &server, true)
            .unwrap();
        indexer.finish_remaining_project_facts();
        workspace_state.analysis_engine.write().resolve();
        let actual = workspace_state
            .analysis_engine
            .read()
            .semantic_result_fingerprint();

        if let Some(expected) = expected {
            assert_eq!(
                actual, expected,
                "parallel worker completion order must not change the file-owned semantic result"
            );
        } else {
            expected = Some(actual);
        }
    }
}

#[test]
fn providerless_project_pass_records_exact_compact_jruby_replay_candidates() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let ordinary_path = root.join("ordinary.rb");
    let alias_path = root.join("alias.rb");
    let proxy_path = root.join("proxy.rb");
    std::fs::write(
        &ordinary_path,
        "module App\n  USER_NAME = user.profile.name\nend\n",
    )
    .unwrap();
    std::fs::write(
        &alias_path,
        "class Imported\n  java_alias :merged, :combine\nend\n",
    )
    .unwrap();
    std::fs::write(&proxy_path, "DEMO = com.example.Demo.new\n").unwrap();

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.collect_project_facts(&server).unwrap();

    assert_eq!(
        indexer.jruby_catalog_sensitive_files(&jruby_provider(&["com/example/Demo"])),
        vec![alias_path, proxy_path],
        "the providerless pass must retain bounded source hints so only actual JRuby \
             catalog consumers are replayed after the exact project catalog arrives"
    );
}

#[test]
fn exact_jruby_provider_replays_only_catalog_sensitive_project_files() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let ordinary_path = root.join("ordinary.rb");
    let import_path = root.join("import.rb");
    std::fs::write(&ordinary_path, "class User\nend\n").unwrap();
    std::fs::write(
        &import_path,
        "java_import 'com.example.Demo'\nDEMO = Demo.new\n",
    )
    .unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.collect_project_facts(&server).unwrap();
    let imported =
        ruby_analysis::core::FullyQualifiedName::try_from("Demo").expect("valid fixture FQN");
    assert!(
        !AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .all_symbol_facts()
            .iter()
            .any(|fact| fact.fqn == imported),
        "the providerless first pass must not invent a Java import alias"
    );

    let provider = Arc::new(
        jruby_provider(&["com/example/Demo"])
            .with_signature_cache_root(root.join("generated-signatures")),
    );
    let replayed = indexer
        .replay_jruby_catalog_sensitive_files(
            FileProcessor::new().with_jruby_import_provider(provider.clone()),
            &server,
        )
        .unwrap();

    assert_eq!(replayed, 1, "ordinary Ruby files must not be replayed");
    assert!(
        AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .all_symbol_facts()
            .iter()
            .any(|fact| fact.fqn == imported),
        "the exact provider pass must replace the Java-sensitive file with its imported alias"
    );
}

#[test]
fn exact_jruby_provider_installed_before_tail_replays_only_active_frontier_files() {
    let workspace = TempDir::new().unwrap();
    let signature_cache = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active_import.rb");
    let tail_path = root.join("b_tail_import.rb");
    std::fs::write(
        &active_path,
        "java_import 'com.example.Active'\nACTIVE = Active.new\n",
    )
    .unwrap();
    std::fs::write(
        &tail_path,
        "java_import 'com.example.Tail'\nTAIL = Tail.new\n",
    )
    .unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .set_navigation_priority_keys(HashSet::from(["aactiveimport".to_string()]), HashSet::new());
    indexer.collect_project_navigation_facts(&server).unwrap();

    let active = FullyQualifiedName::try_from("Active").unwrap();
    let tail = FullyQualifiedName::try_from("Tail").unwrap();
    assert!(
        !AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .all_symbol_facts()
            .iter()
            .any(|fact| fact.fqn == active),
        "the latency frontier must remain providerless"
    );

    let provider = Arc::new(
        jruby_provider(&["com/example/Active", "com/example/Tail"])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
    );
    indexer.install_jruby_import_provider(provider.clone());
    indexer.collect_remaining_project_facts(&server).unwrap();

    {
        let engine = workspace_state.analysis_engine.read();
        let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
        assert!(
            symbols.iter().any(|fact| fact.fqn == tail),
            "the exhaustive tail must be collected once with the exact provider"
        );
        assert!(
            !symbols.iter().any(|fact| fact.fqn == active),
            "the providerless active file must wait for its bounded exact replay"
        );
    }
    assert_eq!(
        indexer.jruby_catalog_sensitive_files(&provider),
        vec![active_path],
        "provider-aware tail files must not enter the replay set"
    );

    let replayed = indexer
        .replay_jruby_catalog_sensitive_files(
            FileProcessor::new().with_jruby_import_provider(provider),
            &server,
        )
        .unwrap();
    assert_eq!(replayed, 1);
    assert!(
        AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .all_symbol_facts()
            .iter()
            .any(|fact| fact.fqn == active),
        "the bounded replay must replace the active file with exact Java facts"
    );
}

#[test]
fn exact_jruby_provider_handoff_between_batches_replays_only_providerless_files() {
    let workspace = TempDir::new().unwrap();
    let signature_cache = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active.rb");
    let first_path = root.join("b_first_import.rb");
    let second_path = root.join("c_second_import.rb");
    std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
    std::fs::write(
        &first_path,
        "java_import 'com.example.First'\nFIRST = First.new\n",
    )
    .unwrap();
    std::fs::write(
        &second_path,
        "java_import 'com.example.Second'\nSECOND = Second.new\n",
    )
    .unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
    indexer.collect_project_navigation_facts(&server).unwrap();
    indexer
        .refresh_exhaustive_semantic_context(&server)
        .unwrap();

    let first_batch = indexer.take_next_remaining_project_files(1);
    assert_eq!(first_batch, vec![first_path.clone()]);
    indexer
        .collect_project_file_batch(&first_batch, &server, false)
        .unwrap();

    let provider = Arc::new(
        jruby_provider(&["com/example/First", "com/example/Second"])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
    );
    indexer.install_jruby_import_provider(provider.clone());

    let second_batch = indexer.take_next_remaining_project_files(1);
    assert_eq!(second_batch, vec![second_path.clone()]);
    indexer
        .collect_project_file_batch(&second_batch, &server, true)
        .unwrap();
    indexer.finish_remaining_project_facts();

    assert_eq!(
        indexer.jruby_catalog_sensitive_files(&provider),
        vec![first_path],
        "only files collected before the provider handoff may enter the replay set"
    );
    let second = FullyQualifiedName::try_from("Second").unwrap();
    assert!(
        AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .all_symbol_facts()
            .iter()
            .any(|fact| fact.fqn == second),
        "the provider-aware batch must expose its Java import before replay"
    );

    let replayed = indexer
        .replay_jruby_catalog_sensitive_files(
            FileProcessor::new().with_jruby_import_provider(provider),
            &server,
        )
        .unwrap();
    assert_eq!(replayed, 1);
    let first = FullyQualifiedName::try_from("First").unwrap();
    assert!(
        AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .all_symbol_facts()
            .iter()
            .any(|fact| fact.fqn == first),
        "the bounded replay must replace the providerless batch with exact Java facts"
    );
}

#[test]
fn exact_jruby_provider_handoff_preserves_generated_signature_facts() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active.rb");
    let first_path = root.join("b_first_import.rb");
    let second_path = root.join("c_second_import.rb");
    std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
    std::fs::write(
        &first_path,
        "java_import 'com.example.First'\nFIRST = First.new\n",
    )
    .unwrap();
    std::fs::write(
        &second_path,
        "java_import 'com.example.Second'\nSECOND = Second.new\n",
    )
    .unwrap();

    let run = |install_before_tail: bool| {
        let signature_cache = TempDir::new().unwrap();
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
        indexer.collect_project_navigation_facts(&server).unwrap();
        indexer
            .refresh_exhaustive_semantic_context(&server)
            .unwrap();

        let provider = Arc::new(
            jruby_provider_with_superclasses(&[
                ("com/example/First", "com/example/Second"),
                ("com/example/Second", "java/lang/Object"),
            ])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        if install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let first_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(first_batch, vec![first_path.clone()]);
        indexer
            .collect_project_file_batch(&first_batch, &server, false)
            .unwrap();
        if !install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let second_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(second_batch, vec![second_path.clone()]);
        indexer
            .collect_project_file_batch(&second_batch, &server, true)
            .unwrap();
        indexer.finish_remaining_project_facts();
        indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server,
            )
            .unwrap();
        workspace_state.analysis_engine.write().resolve();

        let engine = workspace_state.analysis_engine.read();
        let first_signature_path = signature_cache.path().join("com/example/First.rb");
        let first_signature_id = engine.file_id(&first_signature_path).expect(
            "INVARIANT VIOLATED: generated First signature was not indexed. This is a test bug because both schedules import the exact catalog class. Fix: keep the fixture import and signature cache identity aligned.",
        );
        (
            engine
                .semantic_export_fingerprint(first_signature_id)
                .expect(
                    "INVARIANT VIOLATED: generated First signature has no export fingerprint. This is a test bug because every indexed signature enters through replace_facts. Fix: retain the ordinary file-owned signature lifecycle in the fixture.",
                ),
            engine.semantic_result_fingerprint(),
        )
    };

    let exact_before_tail = run(true);
    let handed_off_between_batches = run(false);
    assert_eq!(
        handed_off_between_batches, exact_before_tail,
        "exact JRuby provider readiness timing must not change generated signature facts or the final semantic result"
    );
}

#[test]
fn exact_jruby_provider_handoff_preserves_ordinary_include_diagnostics() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active.rb");
    let ordinary_path = root.join("b_ordinary_include.rb");
    let import_path = root.join("c_import.rb");
    std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
    std::fs::write(
        &ordinary_path,
        "[301, 302].should include last_response.status\n",
    )
    .unwrap();
    std::fs::write(
        &import_path,
        "java_import 'com.example.Imported'\nIMPORTED = Imported.new\n",
    )
    .unwrap();

    let run = |install_before_tail: bool| {
        let signature_cache = TempDir::new().unwrap();
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
        indexer.collect_project_navigation_facts(&server).unwrap();
        indexer
            .refresh_exhaustive_semantic_context(&server)
            .unwrap();

        let provider = Arc::new(
            jruby_provider(&["com/example/Imported"])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        if install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let ordinary_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(ordinary_batch, vec![ordinary_path.clone()]);
        indexer
            .collect_project_file_batch(&ordinary_batch, &server, false)
            .unwrap();
        if !install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let import_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(import_batch, vec![import_path.clone()]);
        indexer
            .collect_project_file_batch(&import_batch, &server, true)
            .unwrap();
        indexer.finish_remaining_project_facts();
        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server,
            )
            .unwrap();
        assert_eq!(
            replayed, 0,
            "an ordinary Ruby include expression must not enter the JRuby replay set"
        );
        workspace_state.analysis_engine.write().resolve();

        let engine = workspace_state.analysis_engine.read();
        let ordinary_id = engine.file_id(&ordinary_path).expect(
            "INVARIANT VIOLATED: ordinary include fixture was not indexed. This is a test bug because the exhaustive batch must register every selected source. Fix: keep the fixture inside the project root and finish the batch.",
        );
        (
            engine.query().diagnostic_facts_in_file(ordinary_id),
            engine.semantic_result_fingerprint(),
        )
    };

    let exact_before_tail = run(true);
    let handed_off_between_batches = run(false);
    assert_eq!(
        handed_off_between_batches, exact_before_tail,
        "provider readiness timing must not reinterpret ordinary Ruby include expressions as JRuby interfaces"
    );
}

#[test]
fn exact_jruby_replay_is_independent_of_exhaustive_batch_boundaries() {
    let workspace = TempDir::new().unwrap();
    let signature_cache = TempDir::new().unwrap();
    let root = workspace.path();
    let first_path = root.join("a_import.rb");
    let second_path = root.join("b_import.rb");
    std::fs::write(
        &first_path,
        "java_import 'com.example.First'\nFIRST = First.new\nFIRST_LATE = LateBound.new\n",
    )
    .unwrap();
    std::fs::write(
        &second_path,
        "java_import 'com.example.Second'\nSECOND = Second.new\nSECOND_LATE = LateBound.new\n",
    )
    .unwrap();

    let late_dependency_uri = Url::from_file_path(root.join("late_dependency.rb")).unwrap();
    let first = FullyQualifiedName::try_from("First").unwrap();
    let second = FullyQualifiedName::try_from("Second").unwrap();
    let mut expected = None;
    for batch_size in [1, 2] {
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.collect_project_navigation_facts(&server).unwrap();
        while indexer.remaining_project_file_count() > 0 {
            let batch = indexer.take_next_remaining_project_files(batch_size);
            let is_last = indexer.remaining_project_file_count() == 0;
            indexer
                .collect_project_file_batch(&batch, &server, is_last)
                .unwrap();
        }
        indexer.finish_remaining_project_facts();

        FileProcessor::new()
            .collect_file_facts_as_deferred_resolution(
                &late_dependency_uri,
                "class LateBound\nend\n",
                &server,
                SourceKind::External,
            )
            .unwrap();
        let provider = Arc::new(
            jruby_provider(&["com/example/First", "com/example/Second"])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server,
            )
            .unwrap();
        assert_eq!(replayed, 2);
        workspace_state.analysis_engine.write().resolve();
        let engine = workspace_state.analysis_engine.read();
        let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
        assert!(symbols.iter().any(|fact| fact.fqn == first));
        assert!(symbols.iter().any(|fact| fact.fqn == second));
        let actual = engine.semantic_result_fingerprint();
        if let Some(expected) = expected {
            assert_eq!(
                actual, expected,
                "exact JRuby replay must not depend on arbitrary exhaustive batch boundaries"
            );
        } else {
            expected = Some(actual);
        }
    }
}
