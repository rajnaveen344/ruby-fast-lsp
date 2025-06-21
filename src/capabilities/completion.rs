use log::info;
use lsp_types::{CompletionParams, CompletionResponse};

use crate::server::RubyLanguageServer;

pub async fn handle_completion(
    _server: &RubyLanguageServer,
    params: CompletionParams,
) -> CompletionResponse {
    info!("Completion request received with params {:?}", params);
    CompletionResponse::Array(vec![])
}
