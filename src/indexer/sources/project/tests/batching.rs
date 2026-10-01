//! Project input partitioning, exhaustive batch scheduling, and batch-independent semantics.

use super::*;

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
