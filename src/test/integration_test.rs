use lsp_types::*;
use std::path::PathBuf;
use tower_lsp::LanguageServer;

use crate::{handlers::request, server::RubyLanguageServer};

/// Helper function to create absolute paths for test fixtures
fn fixture_dir(relative_path: &str) -> PathBuf {
    let root = std::env::current_dir().expect("Failed to get current directory");
    root.join("src")
        .join("test")
        .join("fixtures")
        .join(relative_path)
}

fn fixture_uri(file_name: &str) -> Url {
    Url::from_file_path(fixture_dir(file_name)).unwrap()
}

async fn init_and_open_file(file_name: &str) -> RubyLanguageServer {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let server = RubyLanguageServer::default();
    let params = InitializeParams {
        root_uri: Some(fixture_uri(file_name)),
        ..Default::default()
    };
    let _ = server.initialize(params).await;
    server
}

async fn _init_and_open_folder(_folder_name: &str) -> RubyLanguageServer {
    todo!()
}

/// Test goto definition functionality for class_declaration.rb
#[tokio::test]
async fn test_goto_definition_class_declaration() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 16,
                    character: 12,
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    assert!(res.unwrap().is_some());
}
