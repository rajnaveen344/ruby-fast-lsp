use std::collections::HashSet;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails};

use crate::{
    indexer::entry::entry_kind::EntryKind,
    types::{ruby_document::RubyDocument, scope::LVScope as Scope},
};

pub fn find_variable_completions(
    document: &RubyDocument,
    scope_stack: &[Scope],
) -> Vec<CompletionItem> {
    let mut completions = vec![];
    let mut seen_variables = HashSet::new();

    // Add local variable completions
    for scope in scope_stack.iter().rev() {
        let scope_id = scope.scope_id();
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::Variable { name, .. } = &entry.kind {
                    let var_name = name.name().to_string();
                    if seen_variables.insert(var_name.clone()) {
                        completions.push(CompletionItem {
                            label: var_name,
                            label_details: Some(CompletionItemLabelDetails {
                                detail: Some(" local_variable".to_string()),
                                description: None,
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

    completions
}
