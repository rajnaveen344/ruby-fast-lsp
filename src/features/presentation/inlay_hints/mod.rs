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
//! InlayHintQuery::get_inlay_hints()
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

use crate::features::cursor::EngineQuery;
use crate::server::RubyLanguageServer;
use crate::utils::lsp::text_range;
use ruby_analysis::indexer::{inlay_hints::InlayNodeCollector, RubyDocument};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind as LspInlayHintKind, InlayHintLabel, InlayHintParams, Range,
};

/// Handle `textDocument/inlayHint` once the document's semantic commit is current.
pub async fn handle(
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> LspResult<Option<Vec<InlayHint>>> {
    let uri = params.text_document.uri;
    let range = params.range;
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;

    // Get document content and Arc
    let (content, doc_arc) = {
        let doc_guard = server.documents.read();
        match doc_guard.get(&uri) {
            Some(doc_arc) => {
                let doc = doc_arc.read();
                (doc.content.clone(), doc_arc.clone())
            }
            None => return Ok(Some(Vec::new())),
        }
    };

    // Create query context.
    let query =
        EngineQuery::with_doc_and_engine(doc_arc.clone(), server.analysis_engine_for_uri(&uri));

    // Get document for query
    let document = doc_arc.read();

    // Delegate to query layer
    let hints = query.get_inlay_hints(&document, &range, &content);

    // Convert to LSP format
    let hints = hints.into_iter().map(to_lsp_hint).collect::<Vec<_>>();
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

impl EngineQuery {
    /// Get all inlay hints for a document within the specified range.
    ///
    /// This is the main entry point for inlay hints. It:
    /// 1. Triggers method return type inference for visible methods
    /// 2. Collects relevant AST nodes via InlayNodeCollector
    /// 3. Generates hints from the collected nodes
    pub fn get_inlay_hints(
        &self,
        document: &RubyDocument,
        range: &Range,
        _content: &str,
    ) -> Vec<InlayHintData> {
        // Step 2: Parse AST
        let content = document.analysis_content();
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let range = text_range(document, *range);

        // Step 3: Collect relevant domain nodes
        let collector =
            InlayNodeCollector::new(range.start_byte, range.end_byte, content.as_bytes());
        let nodes = collector.collect(&root);

        // Step 4: Create hint context
        let context = HintContext {
            file_id: document.analysis_file_id(),
            document,
            analysis_engine: self.analysis_engine().cloned(),
        };

        // Step 5: Generate hints from collected nodes
        let mut hints = Vec::new();

        // Structural hints (end labels, implicit returns)
        hints.extend(generate_structural_hints(&nodes, &context));

        // Variable type hints
        hints.extend(generate_variable_type_hints(&nodes, &context));

        // Method return type and parameter hints
        hints.extend(generate_method_hints(&nodes, &context));

        // Chained method call hints (currently placeholder)
        hints.extend(generate_chained_call_hints(&nodes, &context));

        hints
    }
}
