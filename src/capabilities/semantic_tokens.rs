use lazy_static::lazy_static;
use log::info;
use lsp_types::{
    SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensResult, Url,
    WorkDoneProgressOptions,
};
use ruby_prism::Visit;
use std::{collections::HashMap, time::Instant};

use crate::{
    analyzer_prism::visitors::{empty_visitor::EmptyVisitor, token_visitor::TokenVisitor},
    server::RubyLanguageServer,
};

pub const TOKEN_TYPES: [SemanticTokenType; 23] = [
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::ENUM,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::EVENT,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::MACRO,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::REGEXP,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::DECORATOR,
];

lazy_static! {
    pub static ref TOKEN_TYPES_MAP: HashMap<SemanticTokenType, u32> = {
        let mut map = HashMap::new();
        for (index, token_type) in TOKEN_TYPES.iter().enumerate() {
            map.insert(token_type.clone(), index as u32);
        }
        map
    };
}

pub const TOKEN_MODIFIERS: [SemanticTokenModifier; 10] = [
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::MODIFICATION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
];

lazy_static! {
    pub static ref TOKEN_MODIFIERS_MAP: HashMap<SemanticTokenModifier, usize> = {
        let mut map = HashMap::new();
        for (index, token_modifier) in TOKEN_MODIFIERS.iter().enumerate() {
            map.insert(token_modifier.clone(), index);
        }
        map
    };
}

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
    let document = match server.docs.lock().unwrap().get(&uri) {
        Some(doc) => doc.clone(), // Clone the document to avoid holding the lock
        None => {
            info!("Document not found in cache for URI: {}", uri);
            return SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: Vec::new(),
            });
        }
    };

    let content = document.content.clone();
    let parse_result = ruby_prism::parse(content.as_bytes());
    let parse_time = start_time.elapsed();
    info!("Performance: parse took {:?}", parse_time);

    // Pass the document to the visitor
    let mut visitor = TokenVisitor::new(document);
    let root_node = parse_result.node();
    visitor.visit(&root_node);
    let visit_time = start_time.elapsed() - parse_time;
    info!(
        "Performance: token_generation_visitor took {:?}",
        visit_time
    );

    // Performance measurement with empty visitor
    let mut empty_visitor = EmptyVisitor {};
    empty_visitor.visit(&root_node);
    let empty_visit_time = start_time.elapsed() - parse_time - visit_time;
    info!("Performance: empty_visitor took {:?}", empty_visit_time);

    SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: visitor.tokens,
    })
}
