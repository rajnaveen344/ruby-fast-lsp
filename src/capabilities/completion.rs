use log::info;
use lsp_types::{CompletionItemKind, CompletionResponse, Position, Url};

use crate::{
    analyzer_prism::RubyPrismAnalyzer, indexer::entry::entry_kind::EntryKind,
    server::RubyLanguageServer,
};

pub async fn handle_completion(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> CompletionResponse {
    let document = server.get_doc(&uri).unwrap();
    let analyzer = RubyPrismAnalyzer::new(uri, document.content.clone());
    let (_, _, scope_stack) = analyzer.get_identifier(position);

    let scope_id = scope_stack.last().unwrap().scope_id();
    let entries = document.get_local_var_entries(scope_id);

    info!(
        "Searching with scope id: {:?} and found entries {:?}",
        scope_stack.last().unwrap().scope_id(),
        entries
    );

    if let None = entries {
        return CompletionResponse::Array(vec![]);
    }

    let entries = entries.unwrap();

    info!(
        "Found {} local variable entries, entries: {:#?}",
        entries.len(),
        entries
    );

    let mut completions = vec![];
    for entry in entries {
        match &entry.kind {
            EntryKind::Variable { name } => {
                completions.push(lsp_types::CompletionItem {
                    label: name.name().to_string(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    ..Default::default()
                });
            }
            _ => {}
        }
    }

    CompletionResponse::Array(completions)
}
