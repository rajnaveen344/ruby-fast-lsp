use crate::capabilities::semantic_tokens::{TOKEN_MODIFIERS_MAP, TOKEN_TYPES_MAP};
use crate::types::ruby_document::RubyDocument;
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
use ruby_prism::{
    visit_block_local_variable_node, visit_local_variable_and_write_node,
    visit_local_variable_operator_write_node, visit_local_variable_or_write_node,
    visit_local_variable_read_node, visit_local_variable_target_node,
    visit_local_variable_write_node, CallNode, Location, Visit,
};

pub struct TokenVisitor {
    document: RubyDocument,
    pub tokens: Vec<SemanticToken>,
    current_position: (u32, u32),
}

impl TokenVisitor {
    pub fn new(document: RubyDocument) -> Self {
        Self {
            document,
            tokens: Vec::new(),
            current_position: (0, 0),
        }
    }

    fn add_token(
        &mut self,
        location: &Location,
        token_type: SemanticTokenType,
        modifiers: &[SemanticTokenModifier],
    ) {
        let token_type_index = self.token_type_to_index(token_type);
        let modifiers_bitset = self.token_modifiers_to_bitset(modifiers);
        
        // Use RubyDocument's position methods instead of custom conversion
        let start_pos = self.document.offset_to_position(location.start_offset());
        let end_pos = self.document.offset_to_position(location.end_offset());

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

            self.document.content[start..end]
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
        match TOKEN_TYPES_MAP.get(&token_type) {
            Some(&index) => index,
            None => 0,
        }
    }

    fn token_modifiers_to_bitset(&self, modifiers: &[SemanticTokenModifier]) -> u32 {
        modifiers
            .iter()
            .filter_map(|modifier| TOKEN_MODIFIERS_MAP.get(modifier))
            .fold(0, |bitset, &pos| bitset | (1 << pos))
    }
}

impl Visit<'_> for TokenVisitor {
    fn visit_call_node(&mut self, node: &CallNode) {
        if let Some(receiver) = node.receiver() {
            self.visit(&receiver);
        }

        // To produce tokens in the same order as the code written, we add the method token here.
        // Changing the position of this block in this method will result in tokens being out of order.
        if let Some(message_loc) = node.message_loc() {
            let msg = self.document.content[message_loc.start_offset()..message_loc.end_offset()].to_string();
            if msg.starts_with("[") && (msg.ends_with("]") || msg.ends_with("]=")) {
                // "[]" or "[]=" are not method tokens. Do nothing.
                // Eg. hash[:key]; hash['key'] = 1;
            } else {
                self.add_token(&message_loc, SemanticTokenType::METHOD, &[]);
            }
        }

        if let Some(arguments) = node.arguments() {
            self.visit_arguments_node(&arguments);
        }

        if let Some(block) = node.block() {
            self.visit(&block);
        }
    }

    fn visit_local_variable_read_node(&mut self, node: &ruby_prism::LocalVariableReadNode<'_>) {
        self.add_token(&node.location(), SemanticTokenType::VARIABLE, &[]);
        visit_local_variable_read_node(self, node);
    }

    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'_>) {
        self.add_token(
            &node.name_loc(),
            SemanticTokenType::VARIABLE,
            &[SemanticTokenModifier::DECLARATION],
        );
        visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_and_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableAndWriteNode<'_>,
    ) {
        self.add_token(
            &node.name_loc(),
            SemanticTokenType::VARIABLE,
            &[SemanticTokenModifier::DECLARATION],
        );
        visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_or_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableOrWriteNode<'_>,
    ) {
        self.add_token(
            &node.name_loc(),
            SemanticTokenType::VARIABLE,
            &[SemanticTokenModifier::DECLARATION],
        );
        visit_local_variable_or_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableOperatorWriteNode<'_>,
    ) {
        self.add_token(
            &node.name_loc(),
            SemanticTokenType::VARIABLE,
            &[SemanticTokenModifier::DECLARATION],
        );
        visit_local_variable_operator_write_node(self, node);
    }

    fn visit_local_variable_target_node(&mut self, node: &ruby_prism::LocalVariableTargetNode<'_>) {
        self.add_token(
            &node.location(),
            SemanticTokenType::VARIABLE,
            &[SemanticTokenModifier::DECLARATION],
        );
        visit_local_variable_target_node(self, node);
    }

    fn visit_block_local_variable_node(&mut self, node: &ruby_prism::BlockLocalVariableNode<'_>) {
        self.add_token(&node.location(), SemanticTokenType::VARIABLE, &[]);
        visit_block_local_variable_node(self, node);
    }
}
