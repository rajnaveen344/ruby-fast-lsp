//! Code Lens capability — thin adapter over the query layer.
//!
//! Handles server concerns (config check, document lookup) and converts
//! `CodeLensData` from the query layer into LSP `CodeLens` items.

use log::debug;
use tower_lsp::lsp_types::*;

use crate::query::{CodeLensData, IndexQuery};
use crate::server::RubyLanguageServer;

/// Handle CodeLens request for a document.
pub async fn handle_code_lens(
    lang_server: &RubyLanguageServer,
    params: CodeLensParams,
) -> Option<Vec<CodeLens>> {
    let uri = &params.text_document.uri;

    // 1. Config check (server concern).
    let config = lang_server.config.lock();
    if !config.code_lens_modules_enabled.unwrap_or(true) {
        return Some(Vec::new());
    }
    drop(config);

    // 2. Get document content and Arc.
    let (content, doc_arc) = {
        let docs = lang_server.docs.lock();
        let doc_arc = match docs.get(uri) {
            Some(arc) => arc.clone(),
            None => {
                debug!("Document not found for URI: {}", uri);
                return Some(Vec::new());
            }
        };
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    // 3. Create query with document context. Route by URI for multi-workspace.
    let query = IndexQuery::with_doc_and_engine(
        lang_server.index_for_uri(uri),
        doc_arc,
        lang_server.analysis_engine.clone(),
    );

    // 4. Delegate to query layer.
    let lens_data = query.get_code_lenses(uri, &content);

    // 5. Convert Vec<CodeLensData> → Vec<CodeLens>.
    let mut lenses: Vec<CodeLens> = lens_data.into_iter().map(to_lsp_code_lens).collect();
    lenses.extend(
        lang_server
            .extension_registry
            .code_lenses(uri.as_str(), &content),
    );
    Some(lenses)
}

/// Convert a `CodeLensData` into an LSP `CodeLens`.
fn to_lsp_code_lens(data: CodeLensData) -> CodeLens {
    CodeLens {
        range: data.range,
        command: Some(Command {
            title: data.title,
            command: data.command,
            arguments: Some(vec![
                serde_json::to_value(data.uri.as_str()).unwrap(),
                serde_json::to_value(data.target_position).unwrap(),
                serde_json::to_value(data.locations).unwrap(),
            ]),
        }),
        data: None,
    }
}
