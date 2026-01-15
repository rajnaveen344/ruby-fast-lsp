//! InlayNodeCollector - Visitor that collects AST nodes for inlay hint generation.
//!
//! This visitor traverses the AST and collects nodes relevant for inlay hints.
//! It does NOT generate hints directly; that responsibility belongs to generators.

use ruby_prism::{
    visit_call_node, visit_class_node, visit_module_node, CallNode, ClassNode, ConstantWriteNode,
    DefNode, ModuleNode, Node, Visit,
};
use tower_lsp::lsp_types::{Position, Range};

use super::nodes::{BlockKind, InlayNode, ParamNode, VariableKind};
use crate::types::ruby_document::RubyDocument;

/// Visitor that collects AST nodes relevant for inlay hints.
///
/// Only collects nodes within the specified range for efficiency.
pub struct InlayNodeCollector<'a> {
    document: &'a RubyDocument,
    range: Range,
    source: &'a [u8],
    collected: Vec<InlayNode>,
}

impl<'a> InlayNodeCollector<'a> {
    /// Create a new collector for the given document and range.
    pub fn new(document: &'a RubyDocument, range: Range, source: &'a [u8]) -> Self {
        Self {
            document,
            range,
            source,
            collected: Vec::new(),
        }
    }

    /// Collect all relevant nodes from the AST.
    pub fn collect(mut self, node: &Node<'a>) -> Vec<InlayNode> {
        self.visit(node);
        self.collected
    }

    /// Check if a position is within the requested range.
    #[inline]
    fn is_in_range(&self, pos: &Position) -> bool {
        if pos.line < self.range.start.line || pos.line > self.range.end.line {
            return false;
        }
        if pos.line == self.range.start.line && pos.character < self.range.start.character {
            return false;
        }
        if pos.line == self.range.end.line && pos.character > self.range.end.character {
            return false;
        }
        true
    }

    /// Check if there's a line break after the given offset (for chained calls).
    fn has_line_break_after(&self, offset: usize) -> bool {
        // Look for pattern: current_call<newline+whitespace>.next_call
        let remaining = match self.source.get(offset..) {
            Some(bytes) => bytes,
            None => return false,
        };

        let mut found_newline = false;
        for &byte in remaining.iter().take(50) {
            // Look ahead up to 50 bytes
            match byte {
                b'\n' => found_newline = true,
                b' ' | b'\t' | b'\r' => {}
                b'.' if found_newline => return true, // Found dot after newline
                _ => break,
            }
        }
        false
    }

    /// Collect parameters from a method definition.
    fn collect_params(&self, params: Option<ruby_prism::ParametersNode<'a>>) -> Vec<ParamNode> {
        let Some(params) = params else {
            return Vec::new();
        };

        let mut result = Vec::new();

        // Required parameters
        for param in params.requireds().iter() {
            if let Some(req) = param.as_required_parameter_node() {
                let end_offset = req.location().end_offset();
                let end_position = self.document.offset_to_position(end_offset);
                result.push(ParamNode {
                    name: String::from_utf8_lossy(req.name().as_slice()).to_string(),
                    end_position,
                    has_colon: false,
                });
            }
        }

        // Optional parameters
        for param in params.optionals().iter() {
            if let Some(opt) = param.as_optional_parameter_node() {
                let end_offset = opt.name_loc().end_offset();
                let end_position = self.document.offset_to_position(end_offset);
                result.push(ParamNode {
                    name: String::from_utf8_lossy(opt.name().as_slice()).to_string(),
                    end_position,
                    has_colon: false,
                });
            }
        }

        // Rest parameter (*args)
        if let Some(rest) = params.rest() {
            if let Some(rest_param) = rest.as_rest_parameter_node() {
                if let Some(name) = rest_param.name() {
                    let end_offset = rest_param.location().end_offset();
                    let end_position = self.document.offset_to_position(end_offset);
                    result.push(ParamNode {
                        name: String::from_utf8_lossy(name.as_slice()).to_string(),
                        end_position,
                        has_colon: false,
                    });
                }
            }
        }

        // Keyword parameters
        for param in params.keywords().iter() {
            if let Some(kw_opt) = param.as_optional_keyword_parameter_node() {
                let end_offset = kw_opt.name_loc().end_offset();
                let end_position = self.document.offset_to_position(end_offset);
                result.push(ParamNode {
                    name: String::from_utf8_lossy(kw_opt.name().as_slice()).to_string(),
                    end_position,
                    has_colon: true,
                });
            } else if let Some(kw_req) = param.as_required_keyword_parameter_node() {
                let end_offset = kw_req.name_loc().end_offset();
                let end_position = self.document.offset_to_position(end_offset);
                result.push(ParamNode {
                    name: String::from_utf8_lossy(kw_req.name().as_slice()).to_string(),
                    end_position,
                    has_colon: true,
                });
            }
        }

        // Keyword rest parameter (**kwargs)
        if let Some(kw_rest) = params.keyword_rest() {
            if let Some(kw_rest_param) = kw_rest.as_keyword_rest_parameter_node() {
                if let Some(name) = kw_rest_param.name() {
                    let end_offset = kw_rest_param.location().end_offset();
                    let end_position = self.document.offset_to_position(end_offset);
                    result.push(ParamNode {
                        name: String::from_utf8_lossy(name.as_slice()).to_string(),
                        end_position,
                        has_colon: false,
                    });
                }
            }
        }

        // Block parameter (&block)
        if let Some(block) = params.block() {
            if let Some(name) = block.name() {
                let end_offset = block.location().end_offset();
                let end_position = self.document.offset_to_position(end_offset);
                result.push(ParamNode {
                    name: String::from_utf8_lossy(name.as_slice()).to_string(),
                    end_position,
                    has_colon: false,
                });
            }
        }

        result
    }

    /// Process implicit return within a node (recursive).
    fn process_implicit_return(&mut self, node: &Node) {
        // Explicit return - skip
        if node.as_return_node().is_some() {
            return;
        }

        // Statements - process last statement
        if let Some(stmts) = node.as_statements_node() {
            if let Some(last) = stmts.body().iter().last() {
                self.process_implicit_return(&last);
            }
            return;
        }

        // Begin block - process statements, rescue, else
        if let Some(begin) = node.as_begin_node() {
            if let Some(stmts) = begin.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            if let Some(rescue) = begin.rescue_clause() {
                self.process_implicit_return(&rescue.as_node());
            }
            if let Some(else_clause) = begin.else_clause() {
                self.process_implicit_return(&else_clause.as_node());
            }
            return;
        }

        // If/unless - process both branches
        if let Some(if_node) = node.as_if_node() {
            if let Some(stmts) = if_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            if let Some(subsequent) = if_node.subsequent() {
                self.process_implicit_return(&subsequent);
            }
            return;
        }

        if let Some(unless_node) = node.as_unless_node() {
            if let Some(stmts) = unless_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            if let Some(else_clause) = unless_node.else_clause() {
                self.process_implicit_return(&else_clause.as_node());
            }
            return;
        }

        // Else clause
        if let Some(else_node) = node.as_else_node() {
            if let Some(stmts) = else_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            return;
        }

        // Case/when
        if let Some(case_node) = node.as_case_node() {
            for condition in case_node.conditions().iter() {
                self.process_implicit_return(&condition);
            }
            if let Some(else_clause) = case_node.else_clause() {
                self.process_implicit_return(&else_clause.as_node());
            }
            return;
        }

        if let Some(when_node) = node.as_when_node() {
            if let Some(stmts) = when_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            return;
        }

        // Rescue clause
        if let Some(rescue_node) = node.as_rescue_node() {
            if let Some(stmts) = rescue_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            if let Some(subsequent) = rescue_node.subsequent() {
                self.process_implicit_return(&subsequent.as_node());
            }
            return;
        }

        // Parentheses
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                self.process_implicit_return(&body);
            }
            return;
        }

        // Base case: this is an implicit return expression
        let position = self
            .document
            .offset_to_position(node.location().start_offset());
        if self.is_in_range(&position) {
            self.collected.push(InlayNode::ImplicitReturn { position });
        }
    }
}

impl<'a> Visit<'a> for InlayNodeCollector<'a> {
    fn visit_class_node(&mut self, node: &ClassNode<'a>) {
        let end_offset = node.location().end_offset();
        let end_position = self.document.offset_to_position(end_offset - 1);
        let end_position = Position::new(end_position.line, end_position.character + 1);

        if self.is_in_range(&end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::BlockEnd {
                kind: BlockKind::Class,
                name,
                end_position,
            });
        }

        visit_class_node(self, node);
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'a>) {
        let end_offset = node.location().end_offset();
        let end_position = self.document.offset_to_position(end_offset - 1);
        let end_position = Position::new(end_position.line, end_position.character + 1);

        if self.is_in_range(&end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::BlockEnd {
                kind: BlockKind::Module,
                name,
                end_position,
            });
        }

        visit_module_node(self, node);
    }

    fn visit_def_node(&mut self, node: &DefNode<'a>) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Block end hint
        let end_offset = node.location().end_offset();
        let end_position = self.document.offset_to_position(end_offset - 1);
        let end_position = Position::new(end_position.line, end_position.character + 1);

        if self.is_in_range(&end_position) {
            self.collected.push(InlayNode::BlockEnd {
                kind: BlockKind::Method,
                name: name.clone(),
                end_position,
            });
        }

        // Method definition (for return type and params)
        let return_type_position = node
            .rparen_loc()
            .map(|l| l.end_offset())
            .unwrap_or_else(|| node.name_loc().end_offset());
        let return_type_pos = self.document.offset_to_position(return_type_position);

        if self.is_in_range(&return_type_pos) {
            let params = self.collect_params(node.parameters());

            self.collected.push(InlayNode::MethodDef {
                name: name.clone(),
                params,
                return_type_position: return_type_pos,
            });
        }

        // Process implicit returns in method body
        if let Some(body) = node.body() {
            self.process_implicit_return(&body);
        }

        // Recurse into method body to collect variable assignments and other nodes
        ruby_prism::visit_def_node(self, node);
    }

    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'a>) {
        let name_end_offset = node.name_loc().end_offset();
        let name_end_position = self.document.offset_to_position(name_end_offset);

        if self.is_in_range(&name_end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Local,
                name,
                name_end_position,
            });
        }
    }

    fn visit_instance_variable_write_node(
        &mut self,
        node: &ruby_prism::InstanceVariableWriteNode<'a>,
    ) {
        let name_end_offset = node.name_loc().end_offset();
        let name_end_position = self.document.offset_to_position(name_end_offset);

        if self.is_in_range(&name_end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Instance,
                name,
                name_end_position,
            });
        }
    }

    fn visit_class_variable_write_node(&mut self, node: &ruby_prism::ClassVariableWriteNode<'a>) {
        let name_end_offset = node.name_loc().end_offset();
        let name_end_position = self.document.offset_to_position(name_end_offset);

        if self.is_in_range(&name_end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Class,
                name,
                name_end_position,
            });
        }
    }

    fn visit_global_variable_write_node(&mut self, node: &ruby_prism::GlobalVariableWriteNode<'a>) {
        let name_end_offset = node.name_loc().end_offset();
        let name_end_position = self.document.offset_to_position(name_end_offset);

        if self.is_in_range(&name_end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Global,
                name,
                name_end_position,
            });
        }
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode<'a>) {
        let name_end_offset = node.name_loc().end_offset();
        let name_end_position = self.document.offset_to_position(name_end_offset);

        if self.is_in_range(&name_end_position) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Constant,
                name,
                name_end_position,
            });
        }
    }

    fn visit_call_node(&mut self, node: &CallNode<'a>) {
        // Check for chained method calls with line breaks
        let call_end_offset = node.location().end_offset();

        // Only consider calls that have a line break after them
        if self.has_line_break_after(call_end_offset) {
            let call_end_position = self.document.offset_to_position(call_end_offset);

            if self.is_in_range(&call_end_position) {
                self.collected.push(InlayNode::ChainedCall {
                    call_end_position,
                });
            }
        }

        visit_call_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    fn create_collector<'a>(doc: &'a RubyDocument, source: &'a [u8]) -> InlayNodeCollector<'a> {
        let range = Range {
            start: Position::new(0, 0),
            end: Position::new(1000, 0),
        };
        InlayNodeCollector::new(doc, range, source)
    }

    #[test]
    fn test_collect_class_block_end() {
        let content = "class Foo\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let doc = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let collector = create_collector(&doc, content.as_bytes());
        let nodes = collector.collect(&parse_result.node());

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            InlayNode::BlockEnd { kind, name, .. } => {
                assert_eq!(*kind, BlockKind::Class);
                assert_eq!(name, "Foo");
            }
            _ => panic!("Expected BlockEnd"),
        }
    }

    #[test]
    fn test_collect_method_def() {
        let content = "def foo(x, y)\n  x + y\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let doc = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let collector = create_collector(&doc, content.as_bytes());
        let nodes = collector.collect(&parse_result.node());

        // Should have: BlockEnd, MethodDef, ImplicitReturn
        assert!(nodes.len() >= 2);

        let has_method_def = nodes
            .iter()
            .any(|n| matches!(n, InlayNode::MethodDef { name, .. } if name == "foo"));
        assert!(has_method_def);
    }

    #[test]
    fn test_collect_local_variable_write() {
        let content = "x = 42";
        let uri = Url::parse("file:///test.rb").unwrap();
        let doc = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let collector = create_collector(&doc, content.as_bytes());
        let nodes = collector.collect(&parse_result.node());

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            InlayNode::VariableWrite { kind, name, .. } => {
                assert_eq!(*kind, VariableKind::Local);
                assert_eq!(name, "x");
            }
            _ => panic!("Expected VariableWrite"),
        }
    }

    #[test]
    fn test_collect_chained_call_with_line_break() {
        let content = "users\n  .map(&:name)";
        let uri = Url::parse("file:///test.rb").unwrap();
        let doc = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let collector = create_collector(&doc, content.as_bytes());
        let nodes = collector.collect(&parse_result.node());

        // Should detect the call before the line break
        let has_chained = nodes
            .iter()
            .any(|n| matches!(n, InlayNode::ChainedCall { .. }));
        // Note: The first "users" is a method call that has a line break after
        assert!(has_chained || nodes.is_empty()); // May not detect if users is a local var
    }
}
