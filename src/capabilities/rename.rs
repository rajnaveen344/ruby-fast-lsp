//! Rename capability - Rename local variables
//!
//! Uses AST traversal with Prism's `depth` field for reliable scope resolution.
//! This is more robust than stored positions because the parser's own scope
//! resolution is the source of truth.

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    Position, PrepareRenameResponse, Range, RenameParams, TextDocumentPositionParams, TextEdit,
    WorkspaceEdit,
};

use crate::query::analysis_location::locations_for_ranges;
use crate::server::RubyLanguageServer;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::{Identifier, RenameVisitor, RubyDocument, RubyPrismAnalyzer};

pub async fn handle_prepare_rename(
    server: &RubyLanguageServer,
    params: TextDocumentPositionParams,
) -> Option<PrepareRenameResponse> {
    let uri = params.text_document.uri;
    let position = params.position;
    let (content, version) = {
        let docs = server.docs.lock();
        let document = docs.get(&uri)?.read();
        (document.content.clone(), document.version)
    };

    let doc = RubyDocument::new(uri.clone(), content.clone(), version);
    let cursor_offset = doc.position_to_offset(position);
    let parse_result = doc.parse();
    let root = parse_result.node();
    let local_ranges = RenameVisitor::find_rename_targets(doc.clone(), cursor_offset, &root);
    if let Some(range) = local_ranges
        .into_iter()
        .find(|range| range_contains(*range, position))
    {
        return Some(PrepareRenameResponse::Range(range));
    }

    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.clone());
    let (identifier, _, ancestors, _, _) = analyzer.get_identifier(position);
    let Identifier::RubyConstant { iden, .. } = identifier? else {
        return None;
    };
    let analysis_engine = server.analysis_engine_for_uri(&uri);
    let engine = analysis_engine.read();
    let target = AnalysisQuery::new(&engine).constant_rename_target(&iden, &ancestors)?;
    let location = locations_for_ranges(&engine, target.ranges)
        .into_iter()
        .find(|location| location.uri == uri && range_contains(location.range, position))?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: location.range,
        placeholder: target.current_name.to_string(),
    })
}

fn range_contains(range: Range, position: Position) -> bool {
    range.start <= position && position < range.end
}

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
    let parse_result = doc.parse();
    let root = parse_result.node();

    let ranges = RenameVisitor::find_rename_targets(doc.clone(), cursor_offset, &root);

    let mut changes = HashMap::new();
    if !ranges.is_empty() {
        let edits = ranges
            .into_iter()
            .map(|range| TextEdit {
                new_text: new_name.clone(),
                range,
            })
            .collect();
        changes.insert(uri, edits);
    } else {
        let new_constant = RubyConstant::new(&new_name).ok()?;
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.clone());
        let (identifier, _, ancestors, _, _) = analyzer.get_identifier(position);
        let Identifier::RubyConstant { iden, .. } = identifier? else {
            return None;
        };

        let analysis_engine = server.analysis_engine_for_uri(&uri);
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let target = query.constant_rename_target_for_name(&iden, &ancestors, new_constant)?;
        for location in locations_for_ranges(&engine, target.ranges) {
            changes
                .entry(location.uri)
                .or_insert_with(Vec::new)
                .push(TextEdit {
                    new_text: new_name.clone(),
                    range: location.range,
                });
        }
    }

    if changes.is_empty() {
        return None;
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}
