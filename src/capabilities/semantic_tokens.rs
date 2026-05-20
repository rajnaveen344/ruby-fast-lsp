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
    let start_time = Instant::now();

    // Get the document from server cache
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

    let doc_guard = document.read();
    let parse_result = ruby_prism::parse(doc_guard.content.as_bytes());
    let parse_time = start_time.elapsed();
    debug!("[PERF] parse took {:?}", parse_time);

    // Pass the document to the visitor
    let mut visitor = TokenVisitor::new(&doc_guard);
    let root_node = parse_result.node();
    visitor.visit(&root_node);
    let visit_time = start_time.elapsed() - parse_time;
    debug!("[PERF] token_generation_visitor took {:?}", visit_time);

    SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: visitor.tokens,
    })
}
