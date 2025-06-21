use log::info;
use lsp_types::{
    CompletionParams, CompletionResponse, InlayHintOptions, InlayHintServerCapabilities,
    WorkDoneProgressOptions,
};

use crate::server::RubyLanguageServer;

pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

pub async fn handle_completion(
    _server: &RubyLanguageServer,
    params: CompletionParams,
) -> CompletionResponse {
    info!("Completion request received with params {:?}", params);
    CompletionResponse::Array(vec![])
}
