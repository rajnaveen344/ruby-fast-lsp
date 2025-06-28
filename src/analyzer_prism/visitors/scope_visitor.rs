use lsp_types::{Location as LSPLocation, Position, Range};
use ruby_prism::{visit_block_node, visit_class_node, visit_def_node, visit_module_node, Visit};

use crate::{
    analyzer_prism::RubyDocument,
    types::scope::{LVScope, LVScopeKind, LVScopeStack},
};

/// Visitor for finding the scope stack at a specific position
pub struct ScopeVisitor {
    document: RubyDocument,
    position: Position,
    pub scope_stack: LVScopeStack,
}

impl ScopeVisitor {
    pub fn new(document: RubyDocument, position: Position) -> Self {
        let lv_scope = LVScope::new(
            0,
            LSPLocation {
                uri: document.uri.clone(),
                range: Range::new(
                    document.offset_to_position(0),
                    document.offset_to_position(document.content.len()),
                ),
            },
            LVScopeKind::TopLevel,
        );

        Self {
            document,
            position,
            scope_stack: vec![lv_scope],
        }
    }

    fn push_lv_scope(&mut self, scope_id: usize, location: LSPLocation, kind: LVScopeKind) {
        let scope = LVScope::new(scope_id, location, kind);
        self.scope_stack.push(scope);
    }

    fn pop_lv_scope(&mut self) {
        self.scope_stack.pop();
    }
}

impl Visit<'_> for ScopeVisitor {
    fn visit_def_node(&mut self, node: &ruby_prism::DefNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.push_lv_scope(scope_id, body_loc.clone(), LVScopeKind::Method);

        visit_def_node(self, node);

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.pop_lv_scope();
        }
    }

    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let scope_id = self.document.position_to_offset(body_loc.range.start);
                self.push_lv_scope(scope_id, body_loc.clone(), LVScopeKind::Constant);

        visit_class_node(self, node);

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.pop_lv_scope();
        }
    }

    fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let scope_id = self.document.position_to_offset(body_loc.range.start);
                self.push_lv_scope(scope_id, body_loc.clone(), LVScopeKind::Constant);

        visit_module_node(self, node);

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.pop_lv_scope();
        }
    }

    fn visit_block_node(&mut self, node: &ruby_prism::BlockNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.push_lv_scope(scope_id, body_loc.clone(), LVScopeKind::Block);

        visit_block_node(self, node);

        if !(self.position >= body_loc.range.start && self.position <= body_loc.range.end) {
            self.pop_lv_scope();
        }
    }
}
