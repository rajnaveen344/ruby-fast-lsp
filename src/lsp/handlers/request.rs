//! LSP Request Handlers
//!
//! This module contains handlers for LSP requests (messages that require a response).
//! Each handler delegates to the appropriate capability module for the actual logic.

use crate::environment::extensions::{ExtensionStatusParams, ExtensionStatusResponse};
use crate::lsp::capabilities::debug;
use crate::lsp::capabilities::editing::{
    code_actions, completion, formatting, rename, signature_help,
};
use crate::lsp::capabilities::navigation::{namespace_tree, workspace_symbols};
use crate::lsp::capabilities::presentation::{
    code_lens, document_symbols, folding_range, hover, inlay_hints, selection_ranges,
    semantic_tokens,
};
use crate::server::RubyLanguageServer;
use log::{debug, info};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

pub async fn handle_selection_ranges(
    lang_server: &RubyLanguageServer,
    params: SelectionRangeParams,
) -> LspResult<Option<Vec<SelectionRange>>> {
    Ok(selection_ranges::handle_selection_ranges(lang_server, params).await)
}

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

pub async fn handle_document_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentFormattingParams,
) -> LspResult<Option<Vec<TextEdit>>> {
    Ok(formatting::handle_document_formatting(lang_server, params).await)
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

pub async fn handle_hover(
    lang_server: &RubyLanguageServer,
    params: HoverParams,
) -> LspResult<Option<Hover>> {
    Ok(hover::handle_hover(lang_server, params).await)
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
