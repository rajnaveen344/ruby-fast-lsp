use crate::capabilities::semantic_tokens;
use crate::capabilities::{definition, references};
use crate::indexer::events;
use crate::server::RubyLanguageServer;
use log::{info, warn};
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    info!("Initializing Ruby LSP server");

    let mut indexer = lang_server.indexer.lock().await;
    let workspace_folders = params.workspace_folders;

    if let Some(folder) = workspace_folders.and_then(|folders| folders.first().cloned()) {
        info!(
            "Indexing workspace folder using workspace folder: {:?}",
            folder.uri.as_str()
        );
        let _ = events::init_workspace(&mut indexer, folder.uri.clone()).await;
    } else if let Some(root_uri) = params.root_uri {
        info!(
            "Indexing workspace folder using root URI: {:?}",
            root_uri.as_str()
        );
        let _ = events::init_workspace(&mut indexer, root_uri.clone()).await;
    } else {
        warn!("No workspace folder or root URI provided. A workspace folder is required to function properly");
    }

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            semantic_tokens::semantic_tokens_options(),
        )),
        ..ServerCapabilities::default()
    };

    Ok(InitializeResult {
        capabilities,
        ..InitializeResult::default()
    })
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    Ok(())
}

pub async fn handle_goto_definition(
    lang_server: &RubyLanguageServer,
    params: GotoDefinitionParams,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let uri = params
        .text_document_position_params
        .text_document
        .uri
        .clone();
    let position = params.text_document_position_params.position;
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let indexer = lang_server.indexer.lock().await;
    let definition = definition::find_definition_at_position(&indexer, position, &content).await;

    // Convert Vec<Location> to GotoDefinitionResponse
    match definition {
        Some(locations) => {
            if locations.len() == 1 {
                // If there's only one location, return a scalar response
                Ok(Some(GotoDefinitionResponse::Scalar(locations[0].clone())))
            } else {
                // If there are multiple locations, return an array response
                Ok(Some(GotoDefinitionResponse::Array(locations)))
            }
        }
        None => Ok(None),
    }
}

pub async fn handle_references(
    lang_server: &RubyLanguageServer,
    params: ReferenceParams,
) -> LspResult<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;
    let include_declaration = params.context.include_declaration;
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let indexer = lang_server.indexer.lock().await;
    let references = references::find_references_at_position(
        &indexer,
        &uri,
        position,
        &content,
        include_declaration,
    )
    .await;

    Ok(references)
}

pub async fn handle_semantic_tokens_full(
    lang_server: &RubyLanguageServer,
    params: SemanticTokensParams,
) -> LspResult<Option<SemanticTokensResult>> {
    let uri = params.text_document.uri;
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let tokens = semantic_tokens::generate_semantic_tokens(&content);

    match tokens {
        Ok(tokens) => Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        }))),
        Err(e) => {
            if let Some(client) = lang_server.client.clone() {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error generating semantic tokens: {}", e),
                    )
                    .await;
            }
            Ok(None)
        }
    }
}

pub async fn handle_semantic_tokens_range(
    lang_server: &RubyLanguageServer,
    params: SemanticTokensRangeParams,
) -> LspResult<Option<SemanticTokensRangeResult>> {
    let uri = params.text_document.uri;
    let range = params.range;
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();

    let tokens = semantic_tokens::generate_semantic_tokens_for_range(&content, &range);
    match tokens {
        Ok(tokens) => Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        }))),
        Err(e) => {
            if let Some(client) = lang_server.client.clone() {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error generating semantic tokens for range: {}", e),
                    )
                    .await;
            }
            Ok(None)
        }
    }
}
