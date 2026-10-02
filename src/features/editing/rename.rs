//! Rename capability - Rename local variables
//!
//! Uses AST traversal with Prism's `depth` field for reliable scope resolution.
//! This is more robust than stored positions because the parser's own scope
//! resolution is the source of truth.

use log::info;
use std::collections::HashMap;
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;

use tower_lsp::lsp_types::{
    Position, PrepareRenameResponse, Range, RenameParams, TextDocumentPositionParams, TextEdit,
    Url, WorkspaceEdit,
};

use crate::features::cursor::analysis_location::locations_for_ranges;
use crate::features::cursor::{Cursor, EngineQuery};
use crate::server::RubyLanguageServer;
use crate::utils::lsp::{lsp_text_range, source_position};
use ruby_analysis::core::{RubyConstant, RubyMethod, TextRange};
use ruby_analysis::engine::View;
use ruby_analysis::indexer::{Identifier, RenameVisitor, RubyDocument, RubyPrismAnalyzer};

/// Handle `textDocument/prepareRename`.
pub async fn handle_prepare(
    server: &RubyLanguageServer,
    params: TextDocumentPositionParams,
) -> LspResult<Option<PrepareRenameResponse>> {
    info!("Prepare rename request received for: {:?}", params);
    Ok(prepare_rename(server, params))
}

/// Handle `textDocument/rename`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: RenameParams,
) -> LspResult<Option<WorkspaceEdit>> {
    info!(
        "Rename request received for: {:?}",
        params.text_document_position
    );
    let start_time = Instant::now();
    let result = rename(server, params);
    info!("[PERF] Rename completed in {:?}", start_time.elapsed());
    Ok(result)
}

fn prepare_rename(
    server: &RubyLanguageServer,
    params: TextDocumentPositionParams,
) -> Option<PrepareRenameResponse> {
    let uri = params.text_document.uri;
    let document = server.documents.read().get(&uri)?.clone();
    EngineQuery::with_doc_and_engine(document, server.analysis_engine_for_uri(&uri))
        .with_view(|cursor| prepare_rename_at(cursor, &uri, params.position))
}

fn rename(server: &RubyLanguageServer, params: RenameParams) -> Option<WorkspaceEdit> {
    let uri = params.text_document_position.text_document.uri;
    let document = server.documents.read().get(&uri)?.clone();
    EngineQuery::with_doc_and_engine(document, server.analysis_engine_for_uri(&uri)).with_view(
        |cursor| {
            rename_at(
                cursor,
                &uri,
                params.text_document_position.position,
                &params.new_name,
            )
        },
    )
}

/// The renameable range and current name at `position`: a local variable, then
/// a method, then a constant. Ambiguous or external identities fail closed.
pub fn prepare_rename_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let document = cursor.document?;
    let analysis_file_id = document.analysis_file_id();
    let analysis_offset = document.position_to_analysis_offset(source_position(position));

    let doc = RubyDocument::new(uri.clone(), document.content.clone(), document.version);
    let cursor_offset = doc.position_to_offset(source_position(position));
    let parse_result = doc.parse();
    let root = parse_result.node();
    let local_ranges =
        RenameVisitor::find_rename_targets(doc.analysis_file_id(), cursor_offset, &root);
    if let Some(range) = local_ranges
        .into_iter()
        .map(|range| lsp_text_range(&doc, range))
        .find(|range| range_contains(*range, position))
    {
        return Some(PrepareRenameResponse::Range(range));
    }

    let view = cursor.view;
    let (ranges, placeholder) =
        match view.method_rename_target_at(analysis_file_id, analysis_offset) {
            Some(target) => (target.ranges, target.current_name.to_string()),
            None => {
                let analyzer = RubyPrismAnalyzer::new(uri.clone(), document.content.clone());
                let (identifier, _, ancestors, _, _) = analyzer.get_identifier(analysis_offset);
                let Identifier::RubyConstant { iden, .. } = identifier? else {
                    return None;
                };
                let target = view.constant_rename_target(&iden, &ancestors)?;
                (target.ranges, target.current_name.to_string())
            }
        };
    let location = locations_for_ranges(view, ranges)
        .into_iter()
        .find(|location| &location.uri == uri && range_contains(location.range, position))?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: location.range,
        placeholder,
    })
}

fn range_contains(range: Range, position: Position) -> bool {
    range.start <= position && position < range.end
}

/// The edits that rename the local variable, method, or constant at
/// `position` to `new_name`.
pub fn rename_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let document = cursor.document?;
    let content = &document.content;
    let analysis_file_id = document.analysis_file_id();
    let analysis_offset = document.position_to_analysis_offset(source_position(position));

    // Parse and traverse the AST to find all rename targets
    let doc = RubyDocument::new(uri.clone(), content.clone(), 0);
    let cursor_offset = doc.position_to_offset(source_position(position));
    let parse_result = doc.parse();
    let root = parse_result.node();

    let ranges = RenameVisitor::find_rename_targets(doc.analysis_file_id(), cursor_offset, &root);

    let mut changes = HashMap::new();
    if !ranges.is_empty() {
        let edits = ranges
            .into_iter()
            .map(|range| TextEdit {
                new_text: new_name.to_string(),
                range: lsp_text_range(&doc, range),
            })
            .collect();
        changes.insert(uri.clone(), edits);
    } else {
        let view = cursor.view;
        let method_target = RubyMethod::new(new_name).ok().and_then(|new_method| {
            view.method_rename_target_for_name_at(analysis_file_id, analysis_offset, new_method)
        });
        if let Some(target) = method_target {
            push_edits(&mut changes, view, target.ranges, new_name);
        }
        if changes.is_empty() {
            let new_constant = RubyConstant::new(new_name).ok()?;
            let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.clone());
            let (identifier, _, ancestors, _, _) = analyzer.get_identifier(analysis_offset);
            let Identifier::RubyConstant { iden, .. } = identifier? else {
                return None;
            };
            let target = view.constant_rename_target_for_name(&iden, &ancestors, new_constant)?;
            push_edits(&mut changes, view, target.ranges, new_name);
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

fn push_edits(
    changes: &mut HashMap<Url, Vec<TextEdit>>,
    view: &View<'_>,
    ranges: Vec<TextRange>,
    new_name: &str,
) {
    for location in locations_for_ranges(view, ranges) {
        changes.entry(location.uri).or_default().push(TextEdit {
            new_text: new_name.to_string(),
            range: location.range,
        });
    }
}
