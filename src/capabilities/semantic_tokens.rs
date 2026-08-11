use log::{debug, info};
use ruby_prism::Visit;
use std::time::Instant;
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensResult,
    Url, WorkDoneProgressOptions,
};

use crate::server::RubyLanguageServer;
use ruby_analysis::indexer::{
    SemanticTokenData, SemanticTokenKind, SemanticTokenModifierKind, TokenVisitor, TOKEN_MODIFIERS,
    TOKEN_TYPES,
};

pub fn get_semantic_tokens_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        work_done_progress_options: WorkDoneProgressOptions {
            work_done_progress: Some(false),
        },
        legend: SemanticTokensLegend {
            token_types: TOKEN_TYPES.into_iter().map(lsp_token_type).collect(),
            token_modifiers: TOKEN_MODIFIERS
                .into_iter()
                .map(lsp_token_modifier)
                .collect(),
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
        data: visitor.tokens.into_iter().map(lsp_token).collect(),
    })
}

fn lsp_token(token: SemanticTokenData) -> SemanticToken {
    SemanticToken {
        delta_line: token.delta_line,
        delta_start: token.delta_start,
        length: token.length,
        token_type: token.token_type,
        token_modifiers_bitset: token.token_modifiers_bitset,
    }
}

fn lsp_token_type(kind: SemanticTokenKind) -> SemanticTokenType {
    match kind {
        SemanticTokenKind::Namespace => SemanticTokenType::NAMESPACE,
        SemanticTokenKind::Type => SemanticTokenType::TYPE,
        SemanticTokenKind::Class => SemanticTokenType::CLASS,
        SemanticTokenKind::Enum => SemanticTokenType::ENUM,
        SemanticTokenKind::Interface => SemanticTokenType::INTERFACE,
        SemanticTokenKind::Struct => SemanticTokenType::STRUCT,
        SemanticTokenKind::TypeParameter => SemanticTokenType::TYPE_PARAMETER,
        SemanticTokenKind::Parameter => SemanticTokenType::PARAMETER,
        SemanticTokenKind::Variable => SemanticTokenType::VARIABLE,
        SemanticTokenKind::Property => SemanticTokenType::PROPERTY,
        SemanticTokenKind::EnumMember => SemanticTokenType::ENUM_MEMBER,
        SemanticTokenKind::Event => SemanticTokenType::EVENT,
        SemanticTokenKind::Function => SemanticTokenType::FUNCTION,
        SemanticTokenKind::Method => SemanticTokenType::METHOD,
        SemanticTokenKind::Macro => SemanticTokenType::MACRO,
        SemanticTokenKind::Keyword => SemanticTokenType::KEYWORD,
        SemanticTokenKind::Modifier => SemanticTokenType::MODIFIER,
        SemanticTokenKind::Comment => SemanticTokenType::COMMENT,
        SemanticTokenKind::String => SemanticTokenType::STRING,
        SemanticTokenKind::Number => SemanticTokenType::NUMBER,
        SemanticTokenKind::Regexp => SemanticTokenType::REGEXP,
        SemanticTokenKind::Operator => SemanticTokenType::OPERATOR,
        SemanticTokenKind::Decorator => SemanticTokenType::DECORATOR,
    }
}

fn lsp_token_modifier(kind: SemanticTokenModifierKind) -> SemanticTokenModifier {
    match kind {
        SemanticTokenModifierKind::Declaration => SemanticTokenModifier::DECLARATION,
        SemanticTokenModifierKind::Definition => SemanticTokenModifier::DEFINITION,
        SemanticTokenModifierKind::Readonly => SemanticTokenModifier::READONLY,
        SemanticTokenModifierKind::Static => SemanticTokenModifier::STATIC,
        SemanticTokenModifierKind::Deprecated => SemanticTokenModifier::DEPRECATED,
        SemanticTokenModifierKind::Abstract => SemanticTokenModifier::ABSTRACT,
        SemanticTokenModifierKind::Async => SemanticTokenModifier::ASYNC,
        SemanticTokenModifierKind::Modification => SemanticTokenModifier::MODIFICATION,
        SemanticTokenModifierKind::Documentation => SemanticTokenModifier::DOCUMENTATION,
        SemanticTokenModifierKind::DefaultLibrary => SemanticTokenModifier::DEFAULT_LIBRARY,
    }
}
