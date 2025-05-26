use crate::capabilities::semantic_tokens::TOKEN_TYPES;
use crate::{
    analyzer_prism::position::prism_offset_to_lsp_pos,
    capabilities::semantic_tokens::TOKEN_MODIFIERS,
};
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
use ruby_prism::{visit_call_node, CallNode, Location, Visit};

pub struct TokenVisitor {
    code: String,
    pub tokens: Vec<SemanticToken>,
    current_position: (u32, u32),
}

impl TokenVisitor {
    pub fn new(code: String) -> Self {
        Self {
            code,
            tokens: Vec::new(),
            current_position: (0, 0),
        }
    }

    /// Generic method to add a token of any type
    fn add_token(
        &mut self,
        location: &Location,
        token_type: SemanticTokenType,
        modifiers: &[SemanticTokenModifier],
    ) {
        let token_type_index = self.token_type_to_index(token_type);
        let modifiers_bitset = self.token_modifiers_to_bitset(modifiers);
        let start_pos = prism_offset_to_lsp_pos(location.start_offset(), &self.code);
        let end_pos = prism_offset_to_lsp_pos(location.end_offset(), &self.code);

        let delta_line = start_pos.line - self.current_position.0;
        let delta_column = if delta_line == 0 {
            start_pos.character - self.current_position.1
        } else {
            start_pos.character
        };

        // Calculate token length in UTF-16 code units
        let length = if start_pos.line == end_pos.line {
            // If on the same line, just subtract character positions
            end_pos.character - start_pos.character
        } else {
            // For multi-line tokens, we need to count characters
            // This is a simplification - in a real implementation you'd need to handle this case better
            let start = location.start_offset() as usize;
            let end = location.end_offset() as usize;

            self.code[start..end]
                .chars()
                .map(|c| if c.len_utf16() == 2 { 2 } else { 1 })
                .sum::<u32>()
        };

        // Add token to the list
        self.tokens.push(SemanticToken {
            delta_line,
            delta_start: delta_column,
            length,
            token_type: token_type_index,
            token_modifiers_bitset: modifiers_bitset,
        });

        // Update current position
        self.current_position = (start_pos.line, start_pos.character);
    }

    fn token_type_to_index(&self, token_type: SemanticTokenType) -> u32 {
        TOKEN_TYPES
            .iter()
            .position(|t| t == &token_type)
            .map(|pos| pos as u32)
            .unwrap_or(0)
    }

    fn token_modifiers_to_bitset(&self, modifiers: &[SemanticTokenModifier]) -> u32 {
        modifiers
            .iter()
            .filter_map(|modifier| TOKEN_MODIFIERS.iter().position(|m| m == modifier))
            .fold(0, |bitset, pos| bitset | (1 << pos))
    }
}

impl Visit<'_> for TokenVisitor {
    fn visit_call_node(&mut self, node: &CallNode) {
        if let Some(message_loc) = node.message_loc() {
            let msg = self.code[message_loc.start_offset()..message_loc.end_offset()].to_string();
            if msg.starts_with("[") && (msg.ends_with("]") || msg.ends_with("]=")) {
                return;
            }

            self.add_token(&message_loc, SemanticTokenType::METHOD, &[]);
        }

        visit_call_node(self, node);
    }
}
