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
                match &entry.kind {
                    EntryKind::LocalVariable(data) => {
                        let name = &data.name;
                        if seen_variables.insert(name.clone()) {
                            completions.push(CompletionItem {
                                label: name.clone(),
                                label_details: Some(CompletionItemLabelDetails {
                                    detail: Some(" local_variable".to_string()),
                                    description: None,
                                }),
                                kind: Some(CompletionItemKind::VARIABLE),
                                ..Default::default()
                            });
                        }
                    }
                    EntryKind::InstanceVariable(data) => {
                        let name = &data.name;
                        if seen_variables.insert(name.clone()) {
                            completions.push(CompletionItem {
                                label: name.clone(),
                                label_details: Some(CompletionItemLabelDetails {
                                    detail: Some(" instance_variable".to_string()),
                                    description: None,
                                }),
                                kind: Some(CompletionItemKind::VARIABLE),
                                ..Default::default()
                            });
                        }
                    }
                    EntryKind::ClassVariable(data) => {
                        let name = &data.name;
                        if seen_variables.insert(name.clone()) {
                            completions.push(CompletionItem {
                                label: name.clone(),
                                label_details: Some(CompletionItemLabelDetails {
                                    detail: Some(" class_variable".to_string()),
                                    description: None,
                                }),
                                kind: Some(CompletionItemKind::VARIABLE),
                                ..Default::default()
                            });
                        }
                    }
                    EntryKind::GlobalVariable(data) => {
                        let name = &data.name;
                        if seen_variables.insert(name.clone()) {
                            completions.push(CompletionItem {
                                label: name.clone(),
                                label_details: Some(CompletionItemLabelDetails {
                                    detail: Some(" global_variable".to_string()),
                                    description: None,
                                }),
                                kind: Some(CompletionItemKind::VARIABLE),
                                ..Default::default()
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        if scope.kind().is_hard_scope_boundary() {
            break;
        }
    }

    completions
}
