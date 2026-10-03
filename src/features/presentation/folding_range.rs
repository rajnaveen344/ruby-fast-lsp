//! Folding ranges: multi-line classes, modules, methods, control flow,
//! literals, and blocks.

use log::debug;
use ruby_analysis::indexer::RubyDocument;
use ruby_prism::Visit;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams};

use crate::server::Server;

/// Handle `textDocument/foldingRange`.
pub async fn handle(
    server: &Server,
    params: FoldingRangeParams,
) -> LspResult<Option<Vec<FoldingRange>>> {
    let uri = &params.text_document.uri;
    let Some(document) = server.get_doc(uri) else {
        debug!("Document not found for URI: {}", uri);
        return Ok(None);
    };
    let parse_result = document.parse();
    let mut visitor = FoldingRangeVisitor::new(&document);
    visitor.visit(&parse_result.node());
    Ok(Some(visitor.folding_ranges()))
}

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
