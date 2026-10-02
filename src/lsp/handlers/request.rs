//! LSP Request Handlers
//!
//! This module contains handlers for LSP requests (messages that require a response).
//! Each handler delegates to the appropriate capability module for the actual logic.

use crate::environment::extensions::{ExtensionStatusParams, ExtensionStatusResponse};
use crate::lsp::capabilities::debug;
use crate::lsp::capabilities::editing::{
    code_actions, completion, formatting, rename, signature_help,
};
use crate::server::RubyLanguageServer;
use log::{debug, info};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

pub async fn handle_signature_help(
    lang_server: &RubyLanguageServer,
    params: SignatureHelpParams,
) -> LspResult<Option<SignatureHelp>> {
    Ok(signature_help::handle_signature_help(lang_server, params).await)
}

pub async fn handle_code_actions(
    lang_server: &RubyLanguageServer,
    params: CodeActionParams,
) -> LspResult<Option<Vec<CodeActionOrCommand>>> {
    Ok(code_actions::handle_code_actions(lang_server, params).await)
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

pub async fn handle_document_on_type_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentOnTypeFormattingParams,
) -> LspResult<Option<Vec<TextEdit>>> {
    Ok(formatting::handle_document_on_type_formatting(lang_server, params).await)
}

pub async fn handle_document_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentFormattingParams,
) -> LspResult<Option<Vec<TextEdit>>> {
    Ok(formatting::handle_document_formatting(lang_server, params).await)
}

// ============================================================================
// Debug Handlers
// ============================================================================

pub async fn handle_debug_lookup(
    lang_server: &RubyLanguageServer,
    params: debug::LookupParams,
) -> LspResult<debug::LookupResponse> {
    info!("Debug lookup request received for: {}", params.fqn);
    Ok(debug::handle_lookup(lang_server, params))
}

pub async fn handle_export_graph(
    lang_server: &RubyLanguageServer,
    params: debug::ExportGraphParams,
) -> LspResult<debug::ExportGraphResponse> {
    info!("Export graph request received");
    Ok(debug::handle_export_graph(lang_server, params))
}

pub async fn handle_extension_status(
    lang_server: &RubyLanguageServer,
    _params: ExtensionStatusParams,
) -> LspResult<ExtensionStatusResponse> {
    info!("Extension status request received");
    Ok(ExtensionStatusResponse {
        extensions: lang_server.extensions.registry().status_reports(),
    })
}

pub async fn handle_rename(
    lang_server: &RubyLanguageServer,
    params: RenameParams,
) -> LspResult<Option<WorkspaceEdit>> {
    info!(
        "Rename request received for: {:?}",
        params.text_document_position
    );
    let start_time = std::time::Instant::now();
    let result = rename::handle_rename(lang_server, params).await;
    info!("[PERF] Rename completed in {:?}", start_time.elapsed());
    Ok(result)
}

pub async fn handle_prepare_rename(
    lang_server: &RubyLanguageServer,
    params: TextDocumentPositionParams,
) -> LspResult<Option<PrepareRenameResponse>> {
    info!("Prepare rename request received for: {:?}", params);
    Ok(rename::handle_prepare_rename(lang_server, params).await)
}
