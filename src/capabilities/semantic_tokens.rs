use log::{debug, info};
use ruby_prism::Visit;
use std::time::Instant;
use tower_lsp::lsp_types::{
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensResult, Url, WorkDoneProgressOptions,
};

use crate::server::RubyLanguageServer;
use ruby_analysis::indexer::{TokenVisitor, TOKEN_MODIFIERS, TOKEN_TYPES};

pub fn get_semantic_tokens_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        work_done_progress_options: WorkDoneProgressOptions {
            work_done_progress: Some(false),
        },
        legend: SemanticTokensLegend {
            token_types: TOKEN_TYPES.to_vec(),
            token_modifiers: TOKEN_MODIFIERS.to_vec(),
        },
        range: Some(false),
        full: Some(SemanticTokensFullOptions::Bool(true)),
    }
}

pub fn get_semantic_tokens_full(server: &RubyLanguageServer, uri: Url) -> SemanticTokensResult {
    let total_start = Instant::now();

    // Get the document from server cache
    let doc_lookup_start = Instant::now();
    let document = match server.docs.lock().get(&uri) {
        Some(doc) => doc.clone(), // Clone the document to avoid holding the lock
        None => {
            info!("Document not found in cache for URI: {}", uri);
            return SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: Vec::new(),
            });
        }
    };
    let doc_lookup_elapsed = doc_lookup_start.elapsed();

    let doc_guard = document.read();
    let parse_start = Instant::now();
    let parse_result = doc_guard.parse();
    let parse_time = parse_start.elapsed();
    debug!("[PERF] semantic token parse took {:?}", parse_time);

    // Pass the document to the visitor
    let visit_start = Instant::now();
    let mut visitor = TokenVisitor::new(&doc_guard);
    let root_node = parse_result.node();
    visitor.visit(&root_node);
    let visit_time = visit_start.elapsed();
    debug!("[PERF] semantic token visitor took {:?}", visit_time);
    let token_count = visitor.tokens.len();
    info!(
        "[PERF][semanticTokens waterfall] file={} total={:?} doc_lookup={:?} parse={:?} visit={:?} tokens={}",
        uri.path(),
        total_start.elapsed(),
        doc_lookup_elapsed,
        parse_time,
        visit_time,
        token_count
    );

    SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: visitor.tokens,
    })
}
