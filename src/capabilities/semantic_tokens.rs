use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, WorkDoneProgressOptions,
};
use tree_sitter::{Node, Parser, Tree};

// Define token types for our legend - these need to be in the same order as in semantic_tokens_options
const TOKEN_TYPE_NAMESPACE: u32 = 0;
const TOKEN_TYPE_TYPE: u32 = 1;
const TOKEN_TYPE_CLASS: u32 = 2;
const TOKEN_TYPE_ENUM: u32 = 3;
const TOKEN_TYPE_INTERFACE: u32 = 4;
const TOKEN_TYPE_STRUCT: u32 = 5;
const TOKEN_TYPE_TYPE_PARAMETER: u32 = 6;
const TOKEN_TYPE_PARAMETER: u32 = 7;
const TOKEN_TYPE_VARIABLE: u32 = 8;
const TOKEN_TYPE_PROPERTY: u32 = 9;
const TOKEN_TYPE_ENUM_MEMBER: u32 = 10;
const TOKEN_TYPE_EVENT: u32 = 11;
const TOKEN_TYPE_FUNCTION: u32 = 12;
const TOKEN_TYPE_METHOD: u32 = 13;
const TOKEN_TYPE_MACRO: u32 = 14;
const TOKEN_TYPE_KEYWORD: u32 = 15;
const TOKEN_TYPE_MODIFIER: u32 = 16;
const TOKEN_TYPE_COMMENT: u32 = 17;
const TOKEN_TYPE_STRING: u32 = 18;
const TOKEN_TYPE_NUMBER: u32 = 19;
const TOKEN_TYPE_REGEXP: u32 = 20;
const TOKEN_TYPE_OPERATOR: u32 = 21;
const TOKEN_TYPE_DECORATOR: u32 = 22;

pub fn semantic_tokens_options() -> SemanticTokensOptions {
    // Define semantic token types and modifiers for Ruby
    let token_types = vec![
        SemanticTokenType::NAMESPACE,      // 0
        SemanticTokenType::TYPE,           // 1
        SemanticTokenType::CLASS,          // 2
        SemanticTokenType::ENUM,           // 3
        SemanticTokenType::INTERFACE,      // 4
        SemanticTokenType::STRUCT,         // 5
        SemanticTokenType::TYPE_PARAMETER, // 6
        SemanticTokenType::PARAMETER,      // 7
        SemanticTokenType::VARIABLE,       // 8
        SemanticTokenType::PROPERTY,       // 9
        SemanticTokenType::ENUM_MEMBER,    // 10
        SemanticTokenType::EVENT,          // 11
        SemanticTokenType::FUNCTION,       // 12
        SemanticTokenType::METHOD,         // 13
        SemanticTokenType::MACRO,          // 14
        SemanticTokenType::KEYWORD,        // 15
        SemanticTokenType::MODIFIER,       // 16
        SemanticTokenType::COMMENT,        // 17
        SemanticTokenType::STRING,         // 18
        SemanticTokenType::NUMBER,         // 19
        SemanticTokenType::REGEXP,         // 20
        SemanticTokenType::OPERATOR,       // 21
        SemanticTokenType::DECORATOR,      // 22
    ];

    let token_modifiers = vec![
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

    // Create the semantic tokens legend
    let legend = SemanticTokensLegend {
        token_types,
        token_modifiers,
    };

    SemanticTokensOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        legend,
        range: Some(true),
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
    }
}

// Maps tree-sitter node types to LSP semantic token types
fn get_token_type(node_type: &str) -> Option<u32> {
    match node_type {
        "class" => Some(TOKEN_TYPE_CLASS),
        "module" => Some(TOKEN_TYPE_NAMESPACE),
        "method" => Some(TOKEN_TYPE_METHOD),
        "singleton_method" => Some(TOKEN_TYPE_METHOD),
        "identifier" => Some(TOKEN_TYPE_VARIABLE),
        "constant" => Some(TOKEN_TYPE_TYPE),
        "string" => Some(TOKEN_TYPE_STRING),
        "string_literal" => Some(TOKEN_TYPE_STRING),
        "integer" => Some(TOKEN_TYPE_NUMBER),
        "float" => Some(TOKEN_TYPE_NUMBER),
        "comment" => Some(TOKEN_TYPE_COMMENT),
        "instance_variable" => Some(TOKEN_TYPE_PROPERTY),
        "class_variable" => Some(TOKEN_TYPE_PROPERTY),
        "global_variable" => Some(TOKEN_TYPE_VARIABLE),
        "symbol_literal" => Some(TOKEN_TYPE_PROPERTY),
        "regex" => Some(TOKEN_TYPE_REGEXP),
        "hash" => Some(TOKEN_TYPE_STRUCT),
        "array" => Some(TOKEN_TYPE_STRUCT),
        "keyword" => Some(TOKEN_TYPE_KEYWORD),
        _ => None,
    }
}

// Generate semantic tokens from Ruby code
pub fn generate_semantic_tokens(content: &str) -> Result<Vec<SemanticToken>, String> {
    let mut parser = Parser::new();
    let language = tree_sitter_ruby::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|_| "Failed to load Ruby grammar".to_string())?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| "Failed to parse Ruby code".to_string())?;

    let mut tokens = Vec::new();
    let root_node = tree.root_node();

    collect_semantic_tokens(root_node, content, &mut tokens, 0, 0);

    Ok(tokens)
}

// Recursively collect semantic tokens from the tree
fn collect_semantic_tokens(
    node: Node,
    source: &str,
    tokens: &mut Vec<SemanticToken>,
    parent_row: u32,
    parent_col: u32,
) {
    if let Some(token_type) = get_token_type(node.kind()) {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        // Only add tokens for non-empty ranges
        if start_pos.row < end_pos.row
            || (start_pos.row == end_pos.row && start_pos.column < end_pos.column)
        {
            let delta_line = start_pos.row as u32 - parent_row;
            let delta_start = if delta_line == 0 {
                start_pos.column as u32 - parent_col
            } else {
                start_pos.column as u32
            };

            let token_length = if start_pos.row == end_pos.row {
                end_pos.column as u32 - start_pos.column as u32
            } else {
                // Multi-line token, just use the length of the first line
                let line_end = source[node.start_byte()..node.end_byte()]
                    .find('\n')
                    .unwrap_or(node.end_byte() - node.start_byte());
                line_end as u32
            };

            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: token_length,
                token_type,
                token_modifiers_bitset: 0,
            });

            // Update parent position for children
            let new_parent_row = start_pos.row as u32;
            let new_parent_col = start_pos.column as u32;

            // Process child nodes
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    collect_semantic_tokens(child, source, tokens, new_parent_row, new_parent_col);
                }
            }
        }
    } else {
        // Process child nodes if this node doesn't generate a token
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                collect_semantic_tokens(child, source, tokens, parent_row, parent_col);
            }
        }
    }
}
