use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionResponse, Position,
    Url,
};
use std::collections::HashSet;

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
    let (_, _, lv_stack_at_pos) = analyzer.get_identifier(position);

    let mut completions = vec![];
    let mut seen_variables = HashSet::new();

    for scope in lv_stack_at_pos.iter().rev() {
        let scope_id = scope.scope_id();
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::Variable { name } = &entry.kind {
                    let var_name = name.name().to_string();
                    if seen_variables.insert(var_name.clone()) {
                        completions.push(CompletionItem {
                            label: var_name,
                            label_details: Some(CompletionItemLabelDetails {
                                detail: None,
                                description: Some("local_variable".to_string()),
                            }),
                            kind: Some(CompletionItemKind::VARIABLE),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if scope.kind().is_hard_scope_boundary() {
            break;
        }
    }

    CompletionResponse::Array(completions)
}
