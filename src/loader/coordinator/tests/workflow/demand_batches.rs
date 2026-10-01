//! Exact-generation navigation demand consumed by the project batch stream.

use super::*;

#[tokio::test]
async fn project_batch_stream_consumes_an_exact_generation_navigation_demand_first() {
    let fixture = TempDir::new().expect("navigation-demand fixture must be created");
    let project = fixture.path().join("server");
    fs::create_dir_all(&project).expect("project root must be created");
    fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
        .expect("Gemfile must be written");
    let caller_path = project.join("caller.rb");
    let caller_uri = Url::from_file_path(&caller_path).unwrap();
    fs::write(&caller_path, "AccountRecord.lookup\n").unwrap();
    for index in 0..140 {
        fs::write(
            project.join(format!("ordinary_{index:03}.rb")),
            format!("ORDINARY_{index} = {index}\n"),
        )
        .unwrap();
    }
    let target_path = project.join("account_record.rb");
    fs::write(&target_path, "class AccountRecord\nend\n").unwrap();

    let server = create_test_server();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: caller_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "AccountRecord.lookup\n".to_string(),
            },
        },
    )
    .await;
    let run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(
            run.generation(),
            status::IndexingPhase::IndexingProject,
            None,
            None,
        )
        .unwrap();

    let mut coordinator = IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
    coordinator.set_indexing_run(run.clone());
    coordinator
        .setup_file_processor(&server.load_context_for_project(coordinator.workspace_root()));
    coordinator
        .collect_project_navigation_facts(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
            ActiveDocumentPriorityKeys {
                dependency_roots: HashSet::new(),
                project_terminals: vec!["ordinary000".to_string()],
            },
        )
        .await
        .unwrap();
    assert!(
        definitions::find_definition_at_position(&server, caller_uri.clone(), Position::new(0, 2),)
            .await
            .is_none(),
        "the target must remain outside the fixed startup frontier before its demand"
    );
    let ticket = workspace.navigation_demands.request(
        run.generation(),
        navigation_demand::NavigationDemandStage::Project,
        "accountrecord",
    );

    coordinator
        .collect_remaining_project_facts(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
            None,
        )
        .await
        .unwrap();

    assert_eq!(
        ticket.wait().await,
        navigation_demand::NavigationDemandOutcome::TargetProcessed
    );
    let definitions = definitions::definition_locations(
        definitions::find_definition_at_position(&server, caller_uri, Position::new(0, 2))
            .await
            .expect("the exact demanded target must resolve before project-stage completion"),
    );
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        definitions[0].uri,
        Url::from_file_path(target_path).unwrap()
    );
}

#[tokio::test]
async fn project_frontier_consumes_a_bounded_nonpriority_demand() {
    let fixture = TempDir::new().expect("frontier-demand fixture must be created");
    let project = fixture.path().join("server");
    fs::create_dir_all(&project).expect("project root must be created");
    fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
        .expect("Gemfile must be written");
    fs::write(project.join("account.rb"), "class AccountRecord\nend\n").unwrap();
    fs::write(project.join("report.rb"), "class Report\nend\n").unwrap();

    let server = create_test_server();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    let run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(
            run.generation(),
            status::IndexingPhase::IndexingProject,
            None,
            None,
        )
        .unwrap();
    let ticket = workspace.navigation_demands.request(
        run.generation(),
        navigation_demand::NavigationDemandStage::Project,
        "accountrecord",
    );

    let mut coordinator = IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
    coordinator.set_indexing_run(run);
    coordinator
        .setup_file_processor(&server.load_context_for_project(coordinator.workspace_root()));
    coordinator
        .collect_project_navigation_facts(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
            ActiveDocumentPriorityKeys {
                dependency_roots: HashSet::new(),
                project_terminals: vec!["report".to_string()],
            },
        )
        .await
        .unwrap();

    assert_eq!(
        tokio::time::timeout(Duration::from_millis(50), ticket.wait())
            .await
            .expect("the project frontier must consume its bounded demand"),
        navigation_demand::NavigationDemandOutcome::TargetProcessed
    );
}
