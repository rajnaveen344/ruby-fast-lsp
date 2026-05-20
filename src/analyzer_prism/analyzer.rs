use crate::types::{ruby_document::RubyDocument, ruby_namespace::RubyConstant, scope::LVScopeId};
use ruby_prism::Visit;
use tower_lsp::lsp_types::{Position, Url};

use super::identifier::Identifier;
use super::utils;
use super::visitors::identifier_visitor::{IdentifierType, IdentifierVisitor};

/// Main analyzer for Ruby code using Prism
pub struct RubyPrismAnalyzer {
    pub uri: Url,
    pub code: String,
}

impl RubyPrismAnalyzer {
    pub fn new(uri: Url, code: String) -> Self {
        Self { uri, code }
    }

    /// Returns the identifier, identifier type, and the ancestors stack at the time of the lookup.
    pub fn get_identifier(
        &self,
        position: Position,
    ) -> (
        Option<Identifier>,
        Option<IdentifierType>,
        Vec<RubyConstant>,
        LVScopeId,
        ruby_analysis_core::NamespaceKind,
    ) {
        let parse_result = ruby_prism::parse(self.code.as_bytes());
        // Create a RubyDocument with a dummy URI since we only need it for position handling
        let document = RubyDocument::new(self.uri.clone(), self.code.clone(), 0);
        let root_node = parse_result.node();

        let mut iden_visitor = IdentifierVisitor::new(document.clone(), position);
        iden_visitor.visit(&root_node);

        iden_visitor.get_result()
    }

    /// Get the namespace context (enclosing module/class) at a given position.
    pub fn get_namespace_at_position(&self, position: Position) -> Vec<RubyConstant> {
        let parse_result = ruby_prism::parse(self.code.as_bytes());
        let root_node = parse_result.node();

        let mut namespace_stack = Vec::new();
        self.collect_namespaces_containing_position(&root_node, position, &mut namespace_stack);
        namespace_stack
    }

    /// Recursively collect namespace (module/class) names that contain the given position.
    fn collect_namespaces_containing_position(
        &self,
        node: &ruby_prism::Node,
        position: Position,
        namespace_stack: &mut Vec<RubyConstant>,
    ) {
        let position_in_node = |node_loc: &ruby_prism::Location| -> bool {
            let start_offset = node_loc.start_offset();
            let end_offset = node_loc.end_offset();
            let target_offset = self.position_to_offset(position);
            target_offset >= start_offset && target_offset < end_offset
        };

        if let Some(class_node) = node.as_class_node() {
            if position_in_node(&class_node.location()) {
                let constant_path = class_node.constant_path();
                push_constant_path_parts(&constant_path, namespace_stack);

                if let Some(body) = class_node.body() {
                    self.collect_namespaces_containing_position(&body, position, namespace_stack);
                }
                return;
            }
        }

        if let Some(module_node) = node.as_module_node() {
            if position_in_node(&module_node.location()) {
                let constant_path = module_node.constant_path();
                push_constant_path_parts(&constant_path, namespace_stack);

                if let Some(body) = module_node.body() {
                    self.collect_namespaces_containing_position(&body, position, namespace_stack);
                }
                return;
            }
        }

        if let Some(program) = node.as_program_node() {
            for stmt in program.statements().body().iter() {
                self.collect_namespaces_containing_position(&stmt, position, namespace_stack);
            }
        } else if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                self.collect_namespaces_containing_position(&stmt, position, namespace_stack);
            }
        } else if let Some(begin_node) = node.as_begin_node() {
            if let Some(stmts) = begin_node.statements() {
                for stmt in stmts.body().iter() {
                    self.collect_namespaces_containing_position(&stmt, position, namespace_stack);
                }
            }
        }
    }

    /// Convert LSP position to byte offset in the source code.
    fn position_to_offset(&self, position: Position) -> usize {
        let mut offset = 0;
        for (line_idx, line) in self.code.lines().enumerate() {
            if line_idx == position.line as usize {
                return offset + position.character as usize;
            }
            offset += line.len() + 1;
        }
        offset
    }
}

fn push_constant_path_parts(node: &ruby_prism::Node<'_>, namespace_stack: &mut Vec<RubyConstant>) {
    if let Some(cpn) = node.as_constant_path_node() {
        let mut names = Vec::new();
        utils::collect_namespaces(&cpn, &mut names);
        namespace_stack.extend(names);
    } else if let Some(crn) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(crn.name().as_slice());
        if let Ok(constant) = RubyConstant::new(&name) {
            namespace_stack.push(constant);
        }
    }
}
