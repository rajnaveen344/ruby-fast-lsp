use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use ruby_prism::{visit_def_node, Visit};

use crate::types::ruby_document::RubyDocument;

/// Visitor that collects inlay hints for Ruby code
pub struct InlayVisitor<'a> {
    document: &'a RubyDocument,
    inlay_hints: Vec<InlayHint>,
}

impl<'a> InlayVisitor<'a> {
    /// Creates a new inlay visitor for the given document
    pub fn new(document: &'a RubyDocument) -> Self {
        Self {
            document,
            inlay_hints: Vec::new(),
        }
    }

    /// Returns the collected inlay hints
    pub fn inlay_hints(self) -> Vec<InlayHint> {
        self.inlay_hints
    }
}

impl<'a> Visit<'a> for InlayVisitor<'a> {
    fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'a>) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let end_offset = node.location().end_offset();
        // Get the position of last character of the node
        let position = self.document.offset_to_position(end_offset - 1);
        // Add 1 character to position to prevent moving hint to next line
        let position = Position::new(position.line, position.character + 1);
        let hint = InlayHint {
            position,
            label: InlayHintLabel::String(format!("def {}", name)),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: None,
            data: None,
        };

        self.inlay_hints.push(hint);
        visit_def_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::*;

    #[test]
    fn test_inlay_visitor_method_definition() {
        let content = "def foo\n  puts 'Hello'\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = InlayVisitor::new(&document);
        visitor.visit(&node);

        let hints = visitor.inlay_hints();

        assert_eq!(hints.len(), 1);

        let hint = &hints[0];
        assert_eq!(hint.position, Position::new(2, 3));
    }
}
