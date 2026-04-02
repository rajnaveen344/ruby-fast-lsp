//! Rename capability - Rename local variables
//!
//! Uses AST traversal with Prism's `depth` field for reliable scope resolution.
//! This is more robust than stored positions because the parser's own scope
//! resolution is the source of truth.

use std::collections::HashMap;

use tower_lsp::lsp_types::{RenameParams, TextEdit, WorkspaceEdit};

use crate::analyzer_prism::visitors::rename_visitor::RenameVisitor;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;

pub async fn handle_rename(
    server: &RubyLanguageServer,
    params: RenameParams,
) -> Option<WorkspaceEdit> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let new_name = params.new_name;

    // Get document content
    let docs = server.docs.lock();
    let doc_arc = docs.get(&uri)?.clone();
    let document = doc_arc.read();
    let content = document.content.clone();
    drop(docs);

    // Parse and traverse the AST to find all rename targets
    let doc = RubyDocument::new(uri.clone(), content.clone(), 0);
    let cursor_offset = doc.position_to_offset(position);
    let parse_result = ruby_prism::parse(content.as_bytes());
    let root = parse_result.node();

    let ranges = RenameVisitor::find_rename_targets(doc, cursor_offset, &root);

    if ranges.is_empty() {
        return None;
    }

    let edits: Vec<TextEdit> = ranges
        .into_iter()
        .map(|range| TextEdit {
            new_text: new_name.clone(),
            range,
        })
        .collect();

    let mut changes = HashMap::new();
    changes.insert(uri, edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
        ..Default::default()
    })
}
