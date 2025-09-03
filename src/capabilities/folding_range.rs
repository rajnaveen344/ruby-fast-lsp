use crate::types::ruby_document::RubyDocument;
use ruby_prism::Visit;
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams};

/// Visitor that collects folding ranges from Ruby AST nodes
pub struct FoldingRangeVisitor<'a> {
    document: &'a RubyDocument,
    folding_ranges: Vec<FoldingRange>,
}

impl<'a> FoldingRangeVisitor<'a> {
    /// Creates a new folding range visitor
    pub fn new(document: &'a RubyDocument) -> Self {
        Self {
            document,
            folding_ranges: Vec::new(),
        }
    }

    /// Returns the collected folding ranges
    pub fn folding_ranges(self) -> Vec<FoldingRange> {
        self.folding_ranges
    }

    /// Helper method to create a folding range from start and end byte offsets
    fn create_folding_range_from_offsets(
        &mut self,
        start_offset: usize,
        end_offset: usize,
        kind: Option<FoldingRangeKind>,
    ) {
        let start_pos = self.document.offset_to_position(start_offset);
        let end_pos = self.document.offset_to_position(end_offset);

        // Only create folding range if it spans multiple lines
        if end_pos.line > start_pos.line {
            let folding_range = FoldingRange {
                start_line: start_pos.line,
                start_character: None,
                end_line: end_pos.line,
                end_character: None,
                kind,
                collapsed_text: None,
            };
            self.folding_ranges.push(folding_range);
        }
    }
}

impl<'a> Visit<'a> for FoldingRangeVisitor<'a> {
    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_class_node(self, node);
    }

    fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_module_node(self, node);
    }

    fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );
    }

    fn visit_if_node(&mut self, node: &ruby_prism::IfNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_if_node(self, node);
    }

    fn visit_while_node(&mut self, node: &ruby_prism::WhileNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_while_node(self, node);
    }

    fn visit_for_node(&mut self, node: &ruby_prism::ForNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_for_node(self, node);
    }

    fn visit_case_node(&mut self, node: &ruby_prism::CaseNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_case_node(self, node);
    }

    fn visit_begin_node(&mut self, node: &ruby_prism::BeginNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_begin_node(self, node);
    }

    fn visit_array_node(&mut self, node: &ruby_prism::ArrayNode<'a>) {
        // Only fold multi-line arrays
        let start_pos = self
            .document
            .offset_to_position(node.location().start_offset());
        let end_pos = self
            .document
            .offset_to_position(node.location().end_offset());

        if end_pos.line > start_pos.line {
            self.create_folding_range_from_offsets(
                node.location().start_offset(),
                node.location().end_offset(),
                Some(FoldingRangeKind::Region),
            );
        }

        // Continue visiting child nodes
        ruby_prism::visit_array_node(self, node);
    }

    fn visit_hash_node(&mut self, node: &ruby_prism::HashNode<'a>) {
        // Only fold multi-line hashes
        let start_pos = self
            .document
            .offset_to_position(node.location().start_offset());
        let end_pos = self
            .document
            .offset_to_position(node.location().end_offset());

        if end_pos.line > start_pos.line {
            self.create_folding_range_from_offsets(
                node.location().start_offset(),
                node.location().end_offset(),
                Some(FoldingRangeKind::Region),
            );
        }

        // Continue visiting child nodes
        ruby_prism::visit_hash_node(self, node);
    }

    fn visit_block_node(&mut self, node: &ruby_prism::BlockNode<'a>) {
        self.create_folding_range_from_offsets(
            node.location().start_offset(),
            node.location().end_offset(),
            Some(FoldingRangeKind::Region),
        );

        // Continue visiting child nodes
        ruby_prism::visit_block_node(self, node);
    }
}

/// Handles folding range requests for Ruby documents
pub async fn handle_folding_range(
    document: &RubyDocument,
    _params: FoldingRangeParams,
) -> Result<Option<Vec<FoldingRange>>, tower_lsp::jsonrpc::Error> {
    // Parse the Ruby code
    let parse_result = ruby_prism::parse(document.content.as_bytes());
    let node = parse_result.node();

    // Create visitor and collect folding ranges
    let mut visitor = FoldingRangeVisitor::new(document);
    visitor.visit(&node);

    Ok(Some(visitor.folding_ranges()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn test_folding_range_class() {
        let content = "class MyClass\n  def method\n    puts 'hello'\n  end\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = FoldingRangeVisitor::new(&document);
        visitor.visit(&node);

        let ranges = visitor.folding_ranges();
        assert!(!ranges.is_empty());

        // Should have folding ranges for both class and method
        assert!(ranges.len() >= 2);
    }

    #[test]
    fn test_folding_range_control_flow() {
        let content = "if condition\n  puts 'true'\nelse\n  puts 'false'\nend";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = FoldingRangeVisitor::new(&document);
        visitor.visit(&node);

        let ranges = visitor.folding_ranges();
        assert!(!ranges.is_empty());
    }

    #[test]
    fn test_folding_range_multiline_array() {
        let content = "array = [\n  1,\n  2,\n  3\n]";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = FoldingRangeVisitor::new(&document);
        visitor.visit(&node);

        let ranges = visitor.folding_ranges();
        assert!(!ranges.is_empty());
    }

    #[test]
    fn test_folding_range_single_line_no_fold() {
        let content = "puts 'hello world'";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = FoldingRangeVisitor::new(&document);
        visitor.visit(&node);

        let ranges = visitor.folding_ranges();
        // Single line code should not create folding ranges
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_comprehensive_folding_ranges() {
        let content = r#"
class TestClass
  def initialize
    @value = 42
  end
end

if true
  puts "hello"
else
  puts "world"
end

while x < 10
  puts x
  x += 1
end

case value
when 1
  puts "one"
when 2
  puts "two"
end

begin
  risky_operation
rescue
  handle_error
end

array = [
  1,
  2,
  3
]

hash = {
  key: "value",
  another: "data"
}

[1, 2, 3].each do |item|
  puts item
end

for i in 1..5
  puts i
end
"#;

        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = FoldingRangeVisitor::new(&document);
        visitor.visit(&node);

        let ranges = visitor.folding_ranges();

        println!("Found {} folding ranges:", ranges.len());
        for (i, range) in ranges.iter().enumerate() {
            println!("  {}: lines {}-{}", i, range.start_line, range.end_line);
        }

        // We should have folding ranges for:
        // 1. class
        // 2. def (method)
        // 3. if statement
        // 4. while loop
        // 5. case statement
        // 6. begin block
        // 7. array (multi-line)
        // 8. hash (multi-line)
        // 9. block
        // 10. for loop
        assert!(
            ranges.len() >= 10,
            "Expected at least 10 folding ranges, got {}",
            ranges.len()
        );
    }
}
