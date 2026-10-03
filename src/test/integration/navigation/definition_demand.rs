//! Definition requests that arrive while their target is still indexing wait
//! for the exact navigation demand and retry when it completes.

use crate::features::navigation::definition;
use crate::loader::file_processor::FileProcessor;
use crate::loader::scheduling::navigation_demand::NavigationDemandStage;
use crate::loader::scheduling::status::IndexingPhase;
use crate::server::Server;
use std::sync::Arc;
use std::time::Duration;
use tower_lsp::lsp_types::*;

#[tokio::test]
async fn early_definition_request_waits_for_its_exact_project_demand_and_retries() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("server");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();
    let caller_path = project.join("caller.rb");
    let caller_uri = Url::from_file_path(&caller_path).unwrap();
    let caller = "AccountRecord.lookup\n";
    std::fs::write(&caller_path, caller).unwrap();

    let server = Arc::new(Server::default());
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    let run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(run.generation(), IndexingPhase::IndexingProject, None, None)
        .unwrap();
    crate::lsp::lifecycle::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: caller_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: caller.to_string(),
            },
        },
    )
    .await;

    let request_server = server.clone();
    let request_uri = caller_uri.clone();
    let request = tokio::spawn(async move {
        definition::handle(
            &request_server,
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: request_uri },
                    position: Position::new(0, 2),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
    });

    let demanded_keys = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let keys = workspace
                .navigation_demands
                .drain(run.generation(), NavigationDemandStage::Project);
            if !keys.is_empty() {
                break keys;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the early definition request must enqueue a bounded project demand");
    assert_eq!(demanded_keys, vec!["accountrecord".to_string()]);

    let target_path = project.join("account_record.rb");
    let target_uri = Url::from_file_path(&target_path).unwrap();
    std::fs::write(&target_path, "class AccountRecord\nend\n").unwrap();
    let ctx = server.load_context_for_uri(&target_uri);
    FileProcessor::with_extension_registry(server.extension_registry().clone())
        .analyze_file(&target_uri, "class AccountRecord\nend\n", &ctx)
        .unwrap()
        .commit(&ctx);
    workspace.navigation_demands.complete_keys(
        run.generation(),
        NavigationDemandStage::Project,
        &demanded_keys,
    );

    let response = tokio::time::timeout(Duration::from_secs(1), request)
        .await
        .expect("the request must retry immediately after exact demand completion")
        .unwrap()
        .unwrap()
        .expect("the retried definition must resolve");
    let GotoDefinitionResponse::Array(locations) = response else {
        panic!("expected an array definition response");
    };
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, target_uri);
}

#[tokio::test]
async fn dependency_demand_can_resolve_before_the_project_stage_completes() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("server");
    let gem_root = fixture.path().join("gems/bson-4.14.101-java");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(gem_root.join("lib/bson")).unwrap();
    std::fs::write(project.join("Gemfile"), "gem 'bson'\n").unwrap();
    let caller_path = project.join("caller.rb");
    let caller_uri = Url::from_file_path(&caller_path).unwrap();
    let caller = "BSON::ObjectId.new\n";
    std::fs::write(&caller_path, caller).unwrap();

    let server = Arc::new(Server::default());
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    let run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(run.generation(), IndexingPhase::IndexingProject, None, None)
        .unwrap();
    crate::lsp::lifecycle::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: caller_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: caller.to_string(),
            },
        },
    )
    .await;

    let request_server = server.clone();
    let request_uri = caller_uri.clone();
    let request = tokio::spawn(async move {
        definition::handle(
            &request_server,
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: request_uri },
                    position: Position::new(0, 7),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
    });

    let demanded_keys = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let keys = workspace
                .navigation_demands
                .drain(run.generation(), NavigationDemandStage::Dependency);
            if !keys.is_empty() {
                break keys;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the early definition request must enqueue a bounded dependency demand");
    assert_eq!(demanded_keys, vec!["bson".to_string()]);

    let target_path = gem_root.join("lib/bson/object_id.rb");
    let target_uri = Url::from_file_path(&target_path).unwrap();
    let target = "module BSON\n  class ObjectId\n  end\nend\n";
    std::fs::write(&target_path, target).unwrap();
    FileProcessor::with_extension_registry(server.extension_registry().clone())
        .collect_file_facts_as_deferred_resolution_in_engine(
            &target_uri,
            target,
            workspace.handle().load_target(),
            ruby_analysis::core::SourceKind::Gem,
        )
        .unwrap();
    workspace.handle().test_write().resolve();
    workspace.navigation_demands.complete_keys(
        run.generation(),
        NavigationDemandStage::Dependency,
        &demanded_keys,
    );

    let response = tokio::time::timeout(Duration::from_secs(1), request)
        .await
        .expect(
            "the request must use the completed dependency demand without waiting for the \
             still-pending project demand",
        )
        .unwrap()
        .unwrap()
        .expect("the retried dependency definition must resolve");
    let GotoDefinitionResponse::Array(locations) = response else {
        panic!("expected an array definition response");
    };
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, target_uri);
}
