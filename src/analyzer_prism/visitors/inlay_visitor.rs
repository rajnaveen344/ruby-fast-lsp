use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};
use ruby_prism::Visit;

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

        if let Some(body) = node.body() {
            if let Some(statements) = body.as_statements_node() {
                if let Some(last_stmt) = statements.body().iter().last() {
                    let start_offset = last_stmt.location().start_offset();
                    let position = self.document.offset_to_position(start_offset);
                    let hint = InlayHint {
                        position,
                        label: InlayHintLabel::String(format!("return")),
                        kind: Some(InlayHintKind::PARAMETER),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: Some(true),
                        data: None,
                    };

                    self.inlay_hints.push(hint);
                }
            } else if let Some(begin_node) = body.as_begin_node() {
                if let Some(statements) = begin_node.statements() {
                    if let Some(last_stmt) = statements.body().iter().last() {
                        let start_offset = last_stmt.location().start_offset();
                        let position = self.document.offset_to_position(start_offset);
                        let hint = InlayHint {
                            position,
                            label: InlayHintLabel::String(format!("return")),
                            kind: Some(InlayHintKind::PARAMETER),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: Some(true),
                            data: None,
                        };

                        self.inlay_hints.push(hint);
                    }
                }
            }
        }
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

        assert_eq!(hints.len(), 2);

        let hint = &hints[0];
        assert_eq!(hint.position, Position::new(2, 3));
    }

    #[test]
    fn test_inlay_visitor_method_definition_with_begin() {
        let content = "def foo
  puts 'Hello'
rescue => e
  raise e
end";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = InlayVisitor::new(&document);
        visitor.visit(&node);

        let hints = visitor.inlay_hints();

        assert_eq!(hints.len(), 2);

        println!("hints {:?}", hints);

        let hint = &hints[0];
        assert_eq!(hint.position, Position::new(4, 3));

        let hint = &hints[1];
        assert_eq!(hint.position, Position::new(1, 2));
    }
}
