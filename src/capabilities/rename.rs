//! Rename capability - Rename local variables
//!
//! Currently supports:
//! - Local variables (within a single scope)
//!
//! Future enhancements:
//! - ScopeTree for proper variable capture handling
//! - Cross-scope rename (variables captured in blocks)
//! - Other symbol types (constants, methods, etc.)

use tower_lsp::lsp_types::{RenameParams, TextEdit, Url};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::EntryKind;
use crate::server::RubyLanguageServer;

pub async fn handle_rename(
    server: &RubyLanguageServer,
    params: RenameParams,
) -> Option<tower_lsp::lsp_types::WorkspaceEdit> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let new_name = params.new_name;

    // Get document
    let docs = server.docs.lock();
    let doc_arc = docs.get(&uri)?.clone();
    let document = doc_arc.read();

    let content = document.content.clone();
    drop(docs);

    // Analyze the position to find what symbol is at cursor
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content);
    let (identifier_opt, _id_type, _ancestors, _scope_id, _namespace_kind) = analyzer.get_identifier(position);

    let identifier = identifier_opt?;

    // Only support local variables for now
    match identifier {
        Identifier::RubyLocalVariable { name, scope, .. } => {
            let scope_id = scope;
            let var_name = name.to_string();

            // Collect all locations to rename
            let mut locations = Vec::new();

            // Get definition location
            if let Some(entries) = document.get_local_var_entries(scope_id) {
                for entry in entries {
                    if let EntryKind::LocalVariable(data) = &entry.kind {
                        if data.name == var_name {
                            locations.push(entry.location.range);
                            break;
                        }
                    }
                }
            }

            // Get all reference locations
            let refs = document.get_lvar_references(&var_name, &[scope_id]);
            for loc in refs {
                locations.push(loc.range);
            }

            if locations.is_empty() {
                return None;
            }

            // Build the workspace edit
            let edits: Vec<TextEdit> = locations
                .into_iter()
                .map(|range| TextEdit {
                    new_text: new_name.clone(),
                    range,
                })
                .collect();

            let changes = vec![(uri.clone(), edits)]
                .into_iter()
                .collect::<std::collections::HashMap<Url, Vec<TextEdit>>>();

            Some(tower_lsp::lsp_types::WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
                ..Default::default()
            })
        }
        _ => {
            // Not supported - return None for other symbol types
            None
        }
    }
}
