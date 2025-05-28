use crate::capabilities::{definition, references, semantic_tokens};
use crate::server::RubyLanguageServer;
use log::info;
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
    let indexer = lang_server.indexer.lock().await;
    let definition = definition::find_definition_at_position(&indexer, position, &content).await;

    // Convert Vec<Location> to GotoDefinitionResponse
    match definition {
        Some(locations) => {
            if locations.len() == 1 {
                // If there's only one location, return a scalar response
                let location = locations[0].clone();
                info!(
                    "Returning goto definition location: uri={}, range={}:{}-{}:{}",
                    location.uri,
                    location.range.start.line,
                    location.range.start.character,
                    location.range.end.line,
                    location.range.end.character
                );
                Ok(Some(GotoDefinitionResponse::Scalar(location)))
            } else {
                // If there are multiple locations, return an array response
                info!("Returning {} goto definition locations", locations.len());
                for (i, location) in locations.iter().enumerate() {
                    info!(
                        "Location {}: uri={}, range={}:{}-{}:{}",
                        i + 1,
                        location.uri,
                        location.range.start.line,
                        location.range.start.character,
                        location.range.end.line,
                        location.range.end.character
                    );
                }
                Ok(Some(GotoDefinitionResponse::Array(locations)))
            }
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
    _lang_server: &RubyLanguageServer,
    params: SemanticTokensParams,
) -> LspResult<Option<SemanticTokensResult>> {
    let uri = params.text_document.uri.clone();
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let result = semantic_tokens::get_semantic_tokens_full(content);
    Ok(Some(result))
}
