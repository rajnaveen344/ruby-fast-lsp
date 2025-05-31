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
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> Vec<InlayHint> {
    let uri = params.text_document.uri;
    let docs = server.docs.lock().unwrap();
    docs.get(&uri).unwrap().get_inlay_hints()
}
