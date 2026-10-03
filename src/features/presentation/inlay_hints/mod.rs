//! Inlay hints: structural labels, variable and method types, and implicit
//! returns for the requested range.
//!
//! This module provides a clean, principled implementation of inlay hints:
//!
//! 1. `ruby-analysis::indexer::inlay_hints`: AST collection into domain nodes
//! 2. **Generators** (`generators.rs`): Convert nodes to LSP hint data
//!
//! # Architecture
//!
//! ```text
//! LSP Request
//!      │
//!      ▼
//! inlay_hints_at(cursor, range)
//!      │
//!      ├─► Parse AST
//!      │
//!      ├─► ruby_analysis::indexer::inlay_hints::InlayNodeCollector.collect()
//!      │        │
//!      │        └─► Vec<InlayNode>
//!      │
//!      ├─► generate_structural_hints()
//!      ├─► generate_variable_type_hints()
//!      ├─► generate_method_hints()
//!      └─► generate_chained_call_hints()
//!               │
//!               └─► Vec<InlayHintData>
//! ```

mod generators;
mod navigation;
mod tooltip;
mod type_display;

use generators::{
    generate_chained_call_hints, generate_method_hints, generate_structural_hints,
    generate_variable_type_hints, HintContext, InlayHintData, InlayHintKind,
};

use crate::features::cursor::{Cursor, EngineQuery};
use crate::invariant::ExpectInvariant;
use crate::server::Server;
use crate::utils::lsp::text_range;
use ruby_analysis::indexer::inlay_hints::InlayNodeCollector;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind as LspInlayHintKind, InlayHintLabel, InlayHintParams, Range,
};

/// Handle `textDocument/inlayHint` once the document's semantic commit is current.
pub async fn handle(server: &Server, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
    let uri = params.text_document.uri;
    let range = params.range;
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;

    let Some(document) = server.documents.read().get(&uri).cloned() else {
        return Ok(Some(Vec::new()));
    };
    let hints = EngineQuery::with_doc_and_project(document, server.project_for_uri(&uri))
        .with_view(|cursor| inlay_hints_at(cursor, &range))
        .into_iter()
        .map(to_lsp_hint)
        .collect::<Vec<_>>();
    if let Some(project) = server.analysis_workspace_for_uri(&uri) {
        for hint in &hints {
            if let InlayHintLabel::LabelParts(parts) = &hint.label {
                for location in parts.iter().filter_map(|part| part.location.as_ref()) {
                    server.retain_external_document_project(&location.uri, &project);
                }
            }
        }
    }
    Ok(Some(hints))
}

/// Convert InlayHintData to LSP InlayHint.
fn to_lsp_hint(hint: InlayHintData) -> InlayHint {
    InlayHint {
        position: hint.position,
        label: hint.label,
        kind: Some(match hint.kind {
            InlayHintKind::EndLabel | InlayHintKind::ImplicitReturn => LspInlayHintKind::PARAMETER,
            InlayHintKind::VariableType
            | InlayHintKind::MethodReturn
            | InlayHintKind::ParameterType
            | InlayHintKind::ChainedMethodType => LspInlayHintKind::TYPE,
        }),
        text_edits: None,
        tooltip: hint.tooltip,
        padding_left: Some(hint.padding_left),
        padding_right: Some(hint.padding_right),
        data: None,
    }
}

/// Inlay hints for the cursor's document within `range`: structural labels,
/// then variable, method, and chained-call types.
pub fn inlay_hints_at(cursor: Cursor<'_>, range: &Range) -> Vec<InlayHintData> {
    let document = cursor.document.expect_invariant(
        "inlay hints were read without a document",
        "the handler builds its cursor from the open document",
        "construct EngineQuery with with_doc_and_project()",
    );
    let content = document.analysis_content();
    let parse_result = ruby_prism::parse(content.as_bytes());
    let root = parse_result.node();
    let range = text_range(document, *range);
    let nodes = InlayNodeCollector::new(range.start_byte, range.end_byte, content.as_bytes())
        .collect(&root);

    let context = HintContext {
        file_id: document.analysis_file_id(),
        document,
        view: cursor.view,
    };
    let mut hints = generate_structural_hints(&nodes, &context);
    hints.extend(generate_variable_type_hints(&nodes, &context));
    hints.extend(generate_method_hints(&nodes, &context));
    hints.extend(generate_chained_call_hints(&nodes, &context));
    hints
}
