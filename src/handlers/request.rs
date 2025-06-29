use crate::capabilities::{completion, definition, inlay_hints, references, semantic_tokens};
use crate::server::RubyLanguageServer;
use log::{debug, info};
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;

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
        definition::find_definition_at_position(&lang_server, uri, position, &content).await;

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
        completion::handle_completion(lang_server, uri, position).await,
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
