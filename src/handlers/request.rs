use crate::capabilities::semantic_tokens::semantic_tokens_options;
use crate::capabilities::{definition, references};
use crate::server::RubyLanguageServer;
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    if let Some(client) = lang_server.client.clone() {
        client
            .log_message(MessageType::INFO, "Ruby LSP server initialized")
            .await;
    }

    // Store the workspace root
    if let Some(folder) = params
        .workspace_folders
        .and_then(|folders| folders.first().cloned())
    {
        let mut root = lang_server.workspace_root.lock().await;
        *root = Some(folder.uri);
    } else if let Some(root_uri) = params.root_uri {
        let mut root = lang_server.workspace_root.lock().await;
        *root = Some(root_uri);
    }

    if let Some(root_uri) = lang_server.workspace_root.lock().await.clone() {
        if root_uri.scheme() == "file" {
            lang_server.index_file(&root_uri).await;
        } else {
            lang_server.index_workspace_folder(&root_uri).await;
        }
    }

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            semantic_tokens_options(),
        )),
        ..ServerCapabilities::default()
    };

    Ok(InitializeResult {
        capabilities,
        ..InitializeResult::default()
    })
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
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

    // Get document content
    let content = match lang_server.get_document_content(&uri).await {
        Some(content) => content,
        None => return Ok(None),
    };

    // Get indexer reference
    let indexer = lang_server.indexer.lock().await;

    // Use the definition capability
    match definition::find_definition_at_position(&indexer, &uri, position, &content).await {
        Some(location) => Ok(Some(GotoDefinitionResponse::Scalar(location))),
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

    // Get document content
    let content = match lang_server.get_document_content(&uri).await {
        Some(content) => content,
        None => return Ok(None),
    };

    // Get indexer reference
    let indexer = lang_server.indexer.lock().await;

    // Use the references capability
    Ok(references::find_references_at_position(
        &indexer,
        &uri,
        position,
        &content,
        include_declaration,
    )
    .await)
}

pub async fn handle_semantic_tokens_full(
    lang_server: &RubyLanguageServer,
    params: SemanticTokensParams,
) -> LspResult<Option<SemanticTokensResult>> {
    let uri = params.text_document.uri;

    // Get document content from cache
    let content = match lang_server.get_document_content(&uri).await {
        Some(content) => content,
        None => return Ok(None),
    };

    // Use the semantic tokens capability to generate tokens
    match crate::capabilities::semantic_tokens::generate_semantic_tokens(&content) {
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

    // Get document content from cache
    let content = match lang_server.get_document_content(&uri).await {
        Some(content) => content,
        None => return Ok(None),
    };

    // Use the semantic tokens capability to generate tokens for the range
    match crate::capabilities::semantic_tokens::generate_semantic_tokens_for_range(&content, &range)
    {
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
