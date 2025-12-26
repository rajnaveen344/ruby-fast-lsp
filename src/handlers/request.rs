//! LSP Request Handlers
//!
//! This module contains handlers for LSP requests (messages that require a response).
//! Each handler delegates to the appropriate capability module for the actual logic.

use crate::capabilities::{
    code_lens, completion, debug, definitions, document_symbols, folding_range, formatting, hover,
    inlay_hints, namespace_tree, references, semantic_tokens, type_hierarchy, workspace_symbols,
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

    let definition = definitions::find_definition_at_position(lang_server, uri, position).await;

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

pub async fn handle_hover(
    lang_server: &RubyLanguageServer,
    params: HoverParams,
) -> LspResult<Option<Hover>> {
    Ok(hover::handle_hover(lang_server, params).await)
}

// ============================================================================
// Debug Handlers
// ============================================================================

pub async fn handle_list_commands(
    _lang_server: &RubyLanguageServer,
) -> LspResult<debug::ListCommandsResponse> {
    info!("List commands request received");
    Ok(debug::handle_list_commands())
}

pub async fn handle_debug_lookup(
    lang_server: &RubyLanguageServer,
    params: debug::LookupParams,
) -> LspResult<debug::LookupResponse> {
    info!("Debug lookup request received for: {}", params.fqn);
    Ok(debug::handle_lookup(lang_server, params))
}

pub async fn handle_debug_stats(
    lang_server: &RubyLanguageServer,
    _params: debug::StatsParams,
) -> LspResult<debug::StatsResponse> {
    info!("Debug stats request received");
    Ok(debug::handle_stats(lang_server))
}

pub async fn handle_debug_ancestors(
    lang_server: &RubyLanguageServer,
    params: debug::AncestorsParams,
) -> LspResult<debug::AncestorsResponse> {
    info!("Debug ancestors request received for: {}", params.class);
    Ok(debug::handle_ancestors(lang_server, params))
}

pub async fn handle_debug_methods(
    lang_server: &RubyLanguageServer,
    params: debug::MethodsParams,
) -> LspResult<debug::MethodsResponse> {
    info!("Debug methods request received for: {}", params.class);
    Ok(debug::handle_methods(lang_server, params))
}

// ============================================================================
// Type Hierarchy Handlers
// ============================================================================

pub async fn handle_prepare_type_hierarchy(
    lang_server: &RubyLanguageServer,
    params: TypeHierarchyPrepareParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!(
        "Prepare type hierarchy request received for {:?}",
        params
            .text_document_position_params
            .text_document
            .uri
            .path()
    );
    let start_time = std::time::Instant::now();
    let result = type_hierarchy::handle_prepare_type_hierarchy(lang_server, params).await;
    info!(
        "[PERF] Prepare type hierarchy completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

pub async fn handle_supertypes(
    lang_server: &RubyLanguageServer,
    params: TypeHierarchySupertypesParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!("Supertypes request received for: {}", params.item.name);
    let start_time = std::time::Instant::now();
    let result = type_hierarchy::handle_supertypes(lang_server, params).await;
    let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
    info!(
        "[PERF] Supertypes completed in {:?}, returned {} items",
        start_time.elapsed(),
        count
    );
    Ok(result)
}

pub async fn handle_subtypes(
    lang_server: &RubyLanguageServer,
    params: TypeHierarchySubtypesParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!("Subtypes request received for: {}", params.item.name);
    let start_time = std::time::Instant::now();
    let result = type_hierarchy::handle_subtypes(lang_server, params).await;
    let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
    info!(
        "[PERF] Subtypes completed in {:?}, returned {} items",
        start_time.elapsed(),
        count
    );
    Ok(result)
}
