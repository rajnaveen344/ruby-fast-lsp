//! LSP Request Handlers
//!
//! This module contains handlers for LSP requests (messages that require a response).
//! Each handler delegates to the appropriate capability module for the actual logic.

use crate::capabilities::{
    code_lens, completion, definitions, document_symbols, folding_range, formatting, inlay_hints,
    namespace_tree, references, semantic_tokens, workspace_symbols,
};
use crate::server::RubyLanguageServer;
use log::{debug, info};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

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
    let definition =
        definitions::find_definition_at_position(&lang_server, uri, position, &content).await;

    match definition {
        Some(locations) => {
            debug!("Returning {} goto definition locations", locations.len());
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
        None => {
            info!("No definition found for position {:?}", position);
            Ok(None)
        }
    }
}

pub async fn handle_references(
    lang_server: &RubyLanguageServer,
    params: ReferenceParams,
) -> LspResult<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;

    let references = references::find_references_at_position(lang_server, &uri, position).await;

    Ok(references)
}

pub async fn handle_semantic_tokens_full(
    lang_server: &RubyLanguageServer,
    params: SemanticTokensParams,
) -> LspResult<Option<SemanticTokensResult>> {
    Ok(Some(semantic_tokens::get_semantic_tokens_full(
        lang_server,
        params.text_document.uri,
    )))
}

pub async fn handle_inlay_hints(
    lang_server: &RubyLanguageServer,
    params: InlayHintParams,
) -> LspResult<Option<Vec<InlayHint>>> {
    Ok(Some(
        inlay_hints::handle_inlay_hints(lang_server, params).await,
    ))
}

pub async fn handle_completion(
    lang_server: &RubyLanguageServer,
    params: CompletionParams,
) -> LspResult<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;

    debug!("Completion request received with params {:?}", params);

    Ok(Some(
        completion::find_completion_at_position(lang_server, uri, position, params.context).await,
    ))
}

pub async fn handle_completion_resolve(
    _lang_server: &RubyLanguageServer,
    params: CompletionItem,
) -> LspResult<CompletionItem> {
    info!(
        "Completion item resolve request received for {}",
        params.label
    );
    Ok(params)
}

pub async fn handle_document_symbols(
    lang_server: &RubyLanguageServer,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    document_symbols::handle_document_symbols(lang_server, params).await
}

pub async fn handle_workspace_symbols(
    lang_server: &RubyLanguageServer,
    params: WorkspaceSymbolParams,
) -> LspResult<Option<Vec<SymbolInformation>>> {
    Ok(workspace_symbols::handle_workspace_symbols(lang_server, params).await)
}

pub async fn handle_document_on_type_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentOnTypeFormattingParams,
) -> LspResult<Option<Vec<TextEdit>>> {
    Ok(formatting::handle_document_on_type_formatting(lang_server, params).await)
}

pub async fn handle_folding_range(
    lang_server: &RubyLanguageServer,
    params: FoldingRangeParams,
) -> LspResult<Option<Vec<FoldingRange>>> {
    let uri = &params.text_document.uri;

    // Get the document from the language server
    match lang_server.get_doc(uri) {
        Some(document) => folding_range::handle_folding_range(&document, params).await,
        None => {
            debug!("Document not found for URI: {}", uri);
            Ok(None)
        }
    }
}

pub async fn handle_namespace_tree(
    lang_server: &RubyLanguageServer,
    params: namespace_tree::NamespaceTreeParams,
) -> LspResult<namespace_tree::NamespaceTreeResponse> {
    info!("Namespace tree request received");
    let start_time = std::time::Instant::now();
    let result = namespace_tree::handle_namespace_tree(lang_server, params).await;
    info!(
        "[PERF] Namespace tree completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

pub async fn handle_code_lens(
    lang_server: &RubyLanguageServer,
    params: CodeLensParams,
) -> LspResult<Option<Vec<CodeLens>>> {
    info!(
        "CodeLens request received for {:?}",
        params.text_document.uri.path()
    );
    let start_time = std::time::Instant::now();
    let result = code_lens::handle_code_lens(lang_server, params).await;
    info!("[PERF] CodeLens completed in {:?}", start_time.elapsed());
    Ok(result)
}
