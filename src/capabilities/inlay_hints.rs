use log::info;
use lsp_types::{
    InlayHint, InlayHintOptions, InlayHintParams, InlayHintServerCapabilities,
    WorkDoneProgressOptions,
};

use crate::server::RubyLanguageServer;

pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

pub async fn handle_inlay_hints(
    _server: &RubyLanguageServer,
    _params: InlayHintParams,
) -> Vec<InlayHint> {
    info!("Inlay hints request received");
    Vec::new()
}
