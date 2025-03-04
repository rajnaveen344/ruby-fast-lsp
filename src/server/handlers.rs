use anyhow::Result;
use log::{info, warn};
use lsp_types::{
    CompletionResponse, Hover, HoverContents,
    MarkupContent, MarkupKind, Position, Range,
};
use tower_lsp::jsonrpc::Result as LspResult;

use crate::analysis::RubyAnalyzer;
use crate::parser::{document::RubyDocument, RubyParser};

#[derive(Clone)]
pub struct RubyLspHandlers {
    parser: Option<RubyParser>,
    analyzer: RubyAnalyzer,
}

impl RubyLspHandlers {
    pub fn new() -> Result<Self> {
        let parser = match RubyParser::new() {
            Ok(parser) => Some(parser),
            Err(e) => {
                warn!("Failed to initialize Ruby parser: {}", e);
                None
            }
        };
        
        Ok(Self {
            parser,
            analyzer: RubyAnalyzer::new(),
        })
    }

    pub fn handle_hover(&self, document: &RubyDocument, position: Position) -> Option<Hover> {
        info!("Handling hover request at position {:?}", position);
        
        let tree = self.parse_document(document);
        
        // Get hover information
        let hover_info = self
            .analyzer
            .get_hover_info(tree.as_ref(), document.get_content(), position)
            .unwrap_or_else(|| "No information available".to_string());
        
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_info,
            }),
            range: None,
        })
    }

    pub fn handle_completion(
        &self,
        document: &RubyDocument,
        position: Position,
    ) -> CompletionResponse {
        info!("Handling completion request at position {:?}", position);
        
        let tree = self.parse_document(document);
        
        // Get completions from the analyzer
        let items = self.analyzer.get_completions(tree.as_ref(), document.get_content(), position);
        
        CompletionResponse::Array(items)
    }

    pub fn handle_definition(
        &self,
        document: &RubyDocument,
        position: Position,
    ) -> Option<Range> {
        info!("Handling definition request at position {:?}", position);
        
        let tree = self.parse_document(document);
        
        // Find definition
        self.analyzer.find_definition(tree.as_ref(), document.get_content(), position)
    }
    
    // Helper method to parse a document and return the tree
    fn parse_document(&self, document: &RubyDocument) -> Option<tree_sitter::Tree> {
        // If parser is not available, return None
        let parser = match &self.parser {
            Some(parser) => parser,
            None => {
                warn!("Parser not available for document parsing");
                return None;
            }
        };
        
        // Try to parse the document
        match parser.parse(document.get_content()) {
            Some(tree) => {
                info!("Document parsed successfully");
                Some(tree)
            },
            None => {
                warn!("Failed to parse document");
                None
            }
        }
    }
}
