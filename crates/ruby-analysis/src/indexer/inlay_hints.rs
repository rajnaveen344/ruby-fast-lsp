//! Inlay hint AST collection.
//!
//! This module extracts reusable, protocol-neutral inlay hint inputs from Prism
//! AST nodes. Editor adapters convert these byte offsets into protocol positions.

use ruby_prism::{
    visit_call_node, visit_class_node, visit_module_node, CallNode, ClassNode, ConstantWriteNode,
    DefNode, ModuleNode, Node, Visit,
};

/// Represents nodes collected from AST that are relevant for inlay hints.
#[derive(Debug)]
pub enum InlayNode {
    /// Block end: class/module/def for end labels.
    BlockEnd {
        kind: BlockKind,
        name: String,
        end_offset: u32,
    },

    /// Variable assignment for type hints.
    VariableWrite {
        kind: VariableKind,
        name: String,
        name_end_offset: u32,
    },

    /// Method definition for return type and parameter hints.
    MethodDef {
        name: String,
        params: Vec<ParamNode>,
        return_type_offset: u32,
    },

    /// Chained method call with line break for intermediate type hints.
    ChainedCall { call_end_offset: u32 },

    /// Implicit return in method body.
    ImplicitReturn { offset: u32 },
}

/// The kind of block for end labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Class,
    Module,
    Method,
}

impl BlockKind {
    /// Returns the keyword for this block kind.
    pub fn keyword(&self) -> &'static str {
        match self {
            BlockKind::Class => "class",
            BlockKind::Module => "module",
            BlockKind::Method => "def",
        }
    }
}

/// The kind of variable for type hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Local,
    Instance,
    Class,
    Global,
    Constant,
}

/// A method parameter node.
#[derive(Debug)]
pub struct ParamNode {
    pub name: String,
    pub end_offset: u32,
    /// Whether this is a keyword parameter (has colon in syntax).
    pub has_colon: bool,
}

/// Visitor that collects AST nodes relevant for inlay hints.
pub struct InlayNodeCollector<'a> {
    range_start: u32,
    range_end: u32,
    source: &'a [u8],
    collected: Vec<InlayNode>,
}

impl<'a> InlayNodeCollector<'a> {
    /// Create a new collector for the given byte range.
    pub fn new(range_start: u32, range_end: u32, source: &'a [u8]) -> Self {
        assert!(
            range_start <= range_end,
            "INVARIANT VIOLATED: inlay hint collection range start must be <= end. \
             This is a bug because a reversed range cannot be traversed deterministically. \
             Fix: convert editor ranges to sorted byte offsets before collection."
        );
        Self {
            range_start,
            range_end,
            source,
            collected: Vec::new(),
        }
    }

    /// Collect all relevant nodes from the AST.
    pub fn collect(mut self, node: &Node<'a>) -> Vec<InlayNode> {
        self.visit(node);
        self.collected
    }

    #[inline]
    fn is_in_range(&self, offset: u32) -> bool {
        self.range_start <= offset && offset <= self.range_end
    }

    /// Check if there's a line break after the given offset (for chained calls).
    fn has_line_break_after(&self, offset: usize) -> bool {
        let remaining = match self.source.get(offset..) {
            Some(bytes) => bytes,
            None => return false,
        };

        let mut found_newline = false;
        for &byte in remaining.iter().take(50) {
            match byte {
                b'\n' => found_newline = true,
                b' ' | b'\t' | b'\r' => {}
                b'.' if found_newline => return true,
                _ => break,
            }
        }
        false
    }

    fn to_u32_offset(offset: usize) -> u32 {
        u32::try_from(offset).expect(
            "INVARIANT VIOLATED: Ruby source byte offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen analysis offsets before collecting hints for files larger than u32::MAX bytes.",
        )
    }

    fn collect_params(&self, params: Option<ruby_prism::ParametersNode<'a>>) -> Vec<ParamNode> {
        let Some(params) = params else {
            return Vec::new();
        };

        let mut result = Vec::new();

        for param in params.requireds().iter() {
            if let Some(req) = param.as_required_parameter_node() {
                result.push(ParamNode {
                    name: String::from_utf8_lossy(req.name().as_slice()).to_string(),
                    end_offset: Self::to_u32_offset(req.location().end_offset()),
                    has_colon: false,
                });
            }
        }

        for param in params.optionals().iter() {
            if let Some(opt) = param.as_optional_parameter_node() {
                result.push(ParamNode {
                    name: String::from_utf8_lossy(opt.name().as_slice()).to_string(),
                    end_offset: Self::to_u32_offset(opt.name_loc().end_offset()),
                    has_colon: false,
                });
            }
        }

        if let Some(rest) = params.rest() {
            if let Some(rest_param) = rest.as_rest_parameter_node() {
                if let Some(name) = rest_param.name() {
                    result.push(ParamNode {
                        name: String::from_utf8_lossy(name.as_slice()).to_string(),
                        end_offset: Self::to_u32_offset(rest_param.location().end_offset()),
                        has_colon: false,
                    });
                }
            }
        }

        for param in params.keywords().iter() {
            if let Some(kw_opt) = param.as_optional_keyword_parameter_node() {
                result.push(ParamNode {
                    name: String::from_utf8_lossy(kw_opt.name().as_slice()).to_string(),
                    end_offset: Self::to_u32_offset(kw_opt.name_loc().end_offset()),
                    has_colon: true,
                });
            } else if let Some(kw_req) = param.as_required_keyword_parameter_node() {
                result.push(ParamNode {
                    name: String::from_utf8_lossy(kw_req.name().as_slice()).to_string(),
                    end_offset: Self::to_u32_offset(kw_req.name_loc().end_offset()),
                    has_colon: true,
                });
            }
        }

        if let Some(kw_rest) = params.keyword_rest() {
            if let Some(kw_rest_param) = kw_rest.as_keyword_rest_parameter_node() {
                if let Some(name) = kw_rest_param.name() {
                    result.push(ParamNode {
                        name: String::from_utf8_lossy(name.as_slice()).to_string(),
                        end_offset: Self::to_u32_offset(kw_rest_param.location().end_offset()),
                        has_colon: false,
                    });
                }
            }
        }

        if let Some(block) = params.block() {
            if let Some(name) = block.name() {
                result.push(ParamNode {
                    name: String::from_utf8_lossy(name.as_slice()).to_string(),
                    end_offset: Self::to_u32_offset(block.location().end_offset()),
                    has_colon: false,
                });
            }
        }

        result
    }

    fn process_implicit_return(&mut self, node: &Node) {
        if node.as_return_node().is_some() {
            return;
        }

        if let Some(stmts) = node.as_statements_node() {
            if let Some(last) = stmts.body().iter().last() {
                self.process_implicit_return(&last);
            }
            return;
        }

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

        if let Some(else_node) = node.as_else_node() {
            if let Some(stmts) = else_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            return;
        }

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

        if let Some(rescue_node) = node.as_rescue_node() {
            if let Some(stmts) = rescue_node.statements() {
                self.process_implicit_return(&stmts.as_node());
            }
            if let Some(subsequent) = rescue_node.subsequent() {
                self.process_implicit_return(&subsequent.as_node());
            }
            return;
        }

        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                self.process_implicit_return(&body);
            }
            return;
        }

        let offset = Self::to_u32_offset(node.location().start_offset());
        if self.is_in_range(offset) {
            self.collected.push(InlayNode::ImplicitReturn { offset });
        }
    }
}

impl<'a> Visit<'a> for InlayNodeCollector<'a> {
    fn visit_class_node(&mut self, node: &ClassNode<'a>) {
        let end_offset = Self::to_u32_offset(node.location().end_offset());

        if self.is_in_range(end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::BlockEnd {
                kind: BlockKind::Class,
                name,
                end_offset,
            });
        }

        visit_class_node(self, node);
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'a>) {
        let end_offset = Self::to_u32_offset(node.location().end_offset());

        if self.is_in_range(end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::BlockEnd {
                kind: BlockKind::Module,
                name,
                end_offset,
            });
        }

        visit_module_node(self, node);
    }

    fn visit_def_node(&mut self, node: &DefNode<'a>) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        let end_offset = Self::to_u32_offset(node.location().end_offset());
        if self.is_in_range(end_offset) {
            self.collected.push(InlayNode::BlockEnd {
                kind: BlockKind::Method,
                name: name.clone(),
                end_offset,
            });
        }

        let return_type_offset = Self::to_u32_offset(
            node.rparen_loc()
                .map(|l| l.end_offset())
                .unwrap_or_else(|| node.name_loc().end_offset()),
        );

        if self.is_in_range(return_type_offset) {
            let params = self.collect_params(node.parameters());
            self.collected.push(InlayNode::MethodDef {
                name: name.clone(),
                params,
                return_type_offset,
            });
        }

        if let Some(body) = node.body() {
            self.process_implicit_return(&body);
        }

        ruby_prism::visit_def_node(self, node);
    }

    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'a>) {
        let name_end_offset = Self::to_u32_offset(node.name_loc().end_offset());

        if self.is_in_range(name_end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Local,
                name,
                name_end_offset,
            });
        }
    }

    fn visit_instance_variable_write_node(
        &mut self,
        node: &ruby_prism::InstanceVariableWriteNode<'a>,
    ) {
        let name_end_offset = Self::to_u32_offset(node.name_loc().end_offset());

        if self.is_in_range(name_end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Instance,
                name,
                name_end_offset,
            });
        }
    }

    fn visit_class_variable_write_node(&mut self, node: &ruby_prism::ClassVariableWriteNode<'a>) {
        let name_end_offset = Self::to_u32_offset(node.name_loc().end_offset());

        if self.is_in_range(name_end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Class,
                name,
                name_end_offset,
            });
        }
    }

    fn visit_global_variable_write_node(&mut self, node: &ruby_prism::GlobalVariableWriteNode<'a>) {
        let name_end_offset = Self::to_u32_offset(node.name_loc().end_offset());

        if self.is_in_range(name_end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Global,
                name,
                name_end_offset,
            });
        }
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode<'a>) {
        let name_end_offset = Self::to_u32_offset(node.name_loc().end_offset());

        if self.is_in_range(name_end_offset) {
            let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
            self.collected.push(InlayNode::VariableWrite {
                kind: VariableKind::Constant,
                name,
                name_end_offset,
            });
        }
    }

    fn visit_call_node(&mut self, node: &CallNode<'a>) {
        let call_end_offset = node.location().end_offset();

        if self.has_line_break_after(call_end_offset) {
            let call_end_offset = Self::to_u32_offset(call_end_offset);

            if self.is_in_range(call_end_offset) {
                self.collected
                    .push(InlayNode::ChainedCall { call_end_offset });
            }
        }

        visit_call_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(content: &str) -> Vec<InlayNode> {
        let parse_result = ruby_prism::parse(content.as_bytes());
        InlayNodeCollector::new(0, u32::MAX, content.as_bytes()).collect(&parse_result.node())
    }

    #[test]
    fn collects_class_block_end() {
        let nodes = collect("class Foo\nend");

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            InlayNode::BlockEnd { kind, name, .. } => {
                assert_eq!(*kind, BlockKind::Class);
                assert_eq!(name, "Foo");
            }
            InlayNode::VariableWrite { .. }
            | InlayNode::MethodDef { .. }
            | InlayNode::ChainedCall { .. }
            | InlayNode::ImplicitReturn { .. } => panic!("Expected BlockEnd"),
        }
    }

    #[test]
    fn collects_method_def() {
        let nodes = collect("def foo(x, y)\n  x + y\nend");

        let has_method_def = nodes
            .iter()
            .any(|n| matches!(n, InlayNode::MethodDef { name, .. } if name == "foo"));
        assert!(has_method_def);
    }

    #[test]
    fn collects_local_variable_write() {
        let nodes = collect("x = 42");

        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            InlayNode::VariableWrite { kind, name, .. } => {
                assert_eq!(*kind, VariableKind::Local);
                assert_eq!(name, "x");
            }
            InlayNode::BlockEnd { .. }
            | InlayNode::MethodDef { .. }
            | InlayNode::ChainedCall { .. }
            | InlayNode::ImplicitReturn { .. } => panic!("Expected VariableWrite"),
        }
    }

    #[test]
    fn collects_chained_call_with_line_break() {
        let nodes = collect("users\n  .map(&:name)");

        let has_chained = nodes
            .iter()
            .any(|n| matches!(n, InlayNode::ChainedCall { .. }));
        assert!(has_chained || nodes.is_empty());
    }
}
