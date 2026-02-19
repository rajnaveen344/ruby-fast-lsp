use std::collections::HashSet;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Position};

use crate::types::ruby_document::RubyDocument;

pub fn find_variable_completions(
    document: &RubyDocument,
    position: Position,
) -> Vec<CompletionItem> {
    let mut completions = vec![];
    let mut seen_variables = HashSet::new();

    let scope_id = match document.variable_scopes().scope_at_position(position) {
        Some(id) => id,
        None => return completions,
    };

    // Get visible variables from the VariableScopes tree (walks scope chain respecting boundaries)
    let visible_vars = document.variable_scopes().get_visible_variables(scope_id);
    for var in &visible_vars {
        let name = var.name.to_string();
        if seen_variables.insert(name.clone()) {
            completions.push(CompletionItem {
                label: name,
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(" local_variable".to_string()),
                    description: None,
                }),
                kind: Some(CompletionItemKind::VARIABLE),
                ..Default::default()
            });
        }
    }

    completions
}
