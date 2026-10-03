//! Active-document priority and exact navigation demand over the project frontier.

use super::*;

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
fn project_navigation_frontier_releases_before_exhaustive_source_collection() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("user.rb");
    let background_path = root.join("report.rb");
    std::fs::write(&active_path, "class User\nend\n").unwrap();
    std::fs::write(&background_path, "class Report\nend\n").unwrap();

    let server = Server::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["user".to_string()]), HashSet::new());

    indexer
        .collect_project_navigation_facts(
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();

    let user = ruby_analysis::core::RubyConstant::new("User").unwrap();
    let report = ruby_analysis::core::RubyConstant::new("Report").unwrap();
    {
        let engine = workspace_state.handle().test_read();
        let query = engine.view();
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

    indexer
        .collect_remaining_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();

    let engine = workspace_state.handle().test_read();
    assert!(
        !engine
            .view()
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

    let server = Server::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["report".to_string()]), HashSet::new());

    let selection = indexer
        .collect_initial_project_navigation_demand_facts(
            &["accountrecord".to_string()],
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();

    assert_eq!(selection.completed_keys, vec!["accountrecord".to_string()]);
    assert!(selection.deferred_keys.is_empty());
    let user = ruby_analysis::core::RubyConstant::new("AccountRecord").unwrap();
    let report = ruby_analysis::core::RubyConstant::new("Report").unwrap();
    {
        let engine = workspace_state.handle().test_read();
        let query = engine.view();
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

    indexer
        .finish_project_navigation_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();
    let engine = workspace_state.handle().test_read();
    assert!(
        !engine
            .view()
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

    let server = Server::default();
    server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .set_navigation_priority_keys(HashSet::from(["accountrecord".to_string()]), HashSet::new());

    indexer
        .collect_project_navigation_facts(
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();
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
