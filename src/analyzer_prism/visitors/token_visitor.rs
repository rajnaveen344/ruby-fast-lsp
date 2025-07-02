use crate::capabilities::semantic_tokens::{TOKEN_MODIFIERS_MAP, TOKEN_TYPES_MAP};
use crate::types::ruby_document::RubyDocument;
use log::debug;
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
use ruby_prism::{
    visit_block_local_variable_node, visit_constant_path_node, visit_local_variable_and_write_node,
    visit_local_variable_operator_write_node, visit_local_variable_or_write_node,
    visit_local_variable_read_node, visit_local_variable_target_node,
    visit_local_variable_write_node, CallNode, Location, Visit,
};

pub struct TokenVisitor<'a> {
    document: &'a RubyDocument,
    pub tokens: Vec<SemanticToken>,
    current_position: (u32, u32),
}

impl<'a> TokenVisitor<'a> {
    pub fn new(document: &'a RubyDocument) -> Self {
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

        debug!(
            "Adding token for {} at {}:{}-{}:{}",
            &self.document.content
                [location.start_offset() as usize..location.end_offset() as usize],
            start_pos.line,
            start_pos.character,
            end_pos.line,
            end_pos.character
        );

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

    fn is_special_method(&self, method_name: &str) -> bool {
        let special_methods = [
            "<=>",
            "<=",
            ">=",
            "==",
            "===",
            "autoload",
            "autoload?",
            "included_modules",
            "include?",
            "ancestors",
            "attr",
            "attr_reader",
            "attr_writer",
            "attr_accessor",
            "instance_method",
            "instance_methods",
            "freeze",
            "inspect",
            "protected_instance_methods",
            "public_instance_methods",
            "const_missing",
            "undefined_instance_methods",
            "private_instance_methods",
            "class_variables",
            "const_get",
            "class_variable_get",
            "const_defined?",
            "constants",
            "const_set",
            "const_source_location",
            "<",
            "class_variable_defined?",
            "remove_class_variable",
            "private_constant",
            "class_variable_set",
            "deprecate_constant",
            ">",
            "public_constant",
            "include",
            "singleton_class?",
            "prepend",
            "to_s",
            "refinements",
            "define_method",
            "module_exec",
            "class_exec",
            "module_eval",
            "class_eval",
            "name",
            "remove_method",
            "undef_method",
            "alias_method",
            "method_defined?",
            "public_method_defined?",
            "private_method_defined?",
            "protected_method_defined?",
            "public_class_method",
            "private_class_method",
            "public_instance_method",
            "hash",
            "singleton_class",
            "dup",
            "itself",
            "methods",
            "singleton_methods",
            "protected_methods",
            "private_methods",
            "public_methods",
            "instance_variables",
            "instance_variable_get",
            "instance_variable_set",
            "instance_variable_defined?",
            "remove_instance_variable",
            "instance_of?",
            "kind_of?",
            "is_a?",
            "display",
            "public_send",
            "extend",
            "clone",
            "<=>",
            "class",
            "===",
            "!~",
            "frozen?",
            "then",
            "tap",
            "nil?",
            "yield_self",
            "eql?",
            "respond_to?",
            "method",
            "public_method",
            "singleton_method",
            "define_singleton_method",
            "freeze",
            "inspect",
            "object_id",
            "send",
            "to_s",
            "to_enum",
            "enum_for",
            "trap",
            "load",
            "require",
            "require_relative",
            "autoload",
            "autoload?",
            "syscall",
            "open",
            "printf",
            "print",
            "putc",
            "puts",
            "readline",
            "readlines",
            "sprintf",
            "format",
            "Integer",
            "String",
            "Array",
            "Hash",
            "p",
            "exec",
            "exit!",
            "binding",
            "system",
            "spawn",
            "abort",
            "local_variables",
            "Rational",
            "Float",
            "Complex",
            "caller",
            "caller_locations",
            "warn",
            "gets",
            "proc",
            "lambda",
            "raise",
            "fail",
            "global_variables",
            "__method__",
            "__callee__",
            "__dir__",
            "fork",
            "exit",
            "test",
            "set_trace_func",
            "eval",
            "iterator?",
            "block_given?",
            "catch",
            "throw",
            "loop",
            "`",
            "select",
            "sleep",
            "trace_var",
            "untrace_var",
            "at_exit",
            "rand",
            "srand",
            "included",
            "extended",
            "prepended",
            "const_added",
            "method_added",
            "method_removed",
            "remove_const",
            "method_undefined",
            "initialize",
            "initialize_copy",
            "initialize_clone",
            "append_features",
            "extend_object",
            "prepend_features",
            "refine",
            "using",
            "module_function",
            "public",
            "protected",
            "private",
            "ruby2_keywords",
        ];
        special_methods.contains(&method_name)
    }
}

impl Visit<'_> for TokenVisitor<'_> {
    fn visit_call_node(&mut self, node: &CallNode) {
        if let Some(receiver) = node.receiver() {
            self.visit(&receiver);
        }

        // To produce tokens in the same order as the code written, we add the method token here.
        // Changing the position of this block in this method will result in tokens being out of order.
        if let Some(message_loc) = node.message_loc() {
            let msg = self.document.content[message_loc.start_offset()..message_loc.end_offset()]
                .to_string();
            if msg.starts_with("[") && (msg.ends_with("]") || msg.ends_with("]=")) {
                // "[]" or "[]=" are not method tokens. Do nothing.
                // Eg. hash[:key]; hash['key'] = 1;
            } else if self.is_special_method(&msg) {
                self.add_token(&message_loc, SemanticTokenType::MACRO, &[]);
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

    fn visit_if_node(&mut self, node: &ruby_prism::IfNode<'_>) {
        // Determine if this is a modifier-style if (e.g., puts "hello" if condition)
        let is_modifier_style = node.statements().as_ref().map_or(false, |stmts| {
            stmts.location().start_offset() < node.predicate().location().start_offset()
        });

        // Visit nodes in the appropriate order based on style
        if is_modifier_style {
            // Modifier style: statements → predicate → subsequent
            node.statements()
                .map(|stmts| self.visit_statements_node(&stmts));
            self.visit(&node.predicate());
        } else {
            // Regular style: predicate → statements → subsequent
            self.visit(&node.predicate());
            node.statements()
                .map(|stmts| self.visit_statements_node(&stmts));
        }

        // Visit subsequent nodes (else/elsif) if present
        node.subsequent().map(|subsequent| self.visit(&subsequent));
    }

    fn visit_unless_node(&mut self, node: &ruby_prism::UnlessNode<'_>) {
        let is_modifier_style = node.statements().as_ref().map_or(false, |stmts| {
            stmts.location().start_offset() < node.predicate().location().start_offset()
        });

        if is_modifier_style {
            // Modifier style: statements → predicate → subsequent
            node.statements()
                .map(|stmts| self.visit_statements_node(&stmts));
            self.visit(&node.predicate());
        } else {
            // Regular style: predicate → statements → subsequent
            self.visit(&node.predicate());
            node.statements()
                .map(|stmts| self.visit_statements_node(&stmts));
        }

        // Visit subsequent nodes (else/elsif) if present
        node.else_clause()
            .map(|else_clause| self.visit_else_node(&else_clause));
    }

    fn visit_constant_path_node(&mut self, node: &ruby_prism::ConstantPathNode<'_>) {
        visit_constant_path_node(self, node);
        self.add_token(&node.name_loc(), SemanticTokenType::CLASS, &[]);
    }

    fn visit_constant_read_node(&mut self, node: &ruby_prism::ConstantReadNode<'_>) {
        self.add_token(&node.location(), SemanticTokenType::CLASS, &[]);
    }
}
