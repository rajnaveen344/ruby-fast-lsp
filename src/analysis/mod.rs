use anyhow::Result;
use log::{info, warn};
use lsp_types::{Position, Range, CompletionItem, CompletionItemKind, Url};
use tree_sitter::{Tree, Node};
use crate::workspace::WorkspaceManager;
use crate::parser::RubyParser;

#[derive(Clone)]
pub struct RubyAnalyzer {
    // Add fields as needed
}

impl RubyAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn analyze(&self, tree: Option<&Tree>, _source_code: &str) -> Result<()> {
        if let Some(tree) = tree {
            info!("Analyzing Ruby code with tree-sitter");
            let root_node = tree.root_node();
            info!("Root node kind: {}", root_node.kind());
            info!("Root node has {} children", root_node.child_count());

            // Add more analysis as needed

            Ok(())
        } else {
            warn!("Cannot analyze: tree not available");
            Ok(()) // Return Ok to avoid crashing the server
        }
    }

    pub fn find_definition(
        &self, 
        tree: Option<&Tree>, 
        source_code: &str, 
        position: Position,
        workspace: Option<&WorkspaceManager>,
        uri: &Url
    ) -> Option<Range> {
        // First try to find definition in the current file
        if let Some(range) = self.find_definition_in_file(tree, source_code, position) {
            return Some(range);
        }

        // If not found in current file and we have a workspace, search across workspace
        if let Some(workspace) = workspace {
            info!("Definition not found in current file, searching workspace");
            return self.find_definition_in_workspace(workspace, uri, source_code, position);
        }

        None
    }

    // Find definition within the current file
    fn find_definition_in_file(&self, tree: Option<&Tree>, source_code: &str, position: Position) -> Option<Range> {
        let tree = tree?;

        // Find the node at the current position
        let node = self.node_at_position(tree, position, source_code)?;

        info!("Looking for definition of node kind: {}", node.kind());

        // Based on the node type, find the appropriate definition
        match node.kind() {
            "identifier" => {
                // For identifiers, try to find where they are defined
                let identifier_text = self.get_node_text(&node, source_code);
                info!("Looking for definition of identifier: {}", identifier_text);

                // Search for method definitions, variable assignments, etc.
                let root_node = tree.root_node();
                let mut cursor = root_node.walk();

                // This is a simplified approach - in a real implementation,
                // you would need to handle scopes, inheritance, etc.
                for child in root_node.children(&mut cursor) {
                    if child.kind() == "method" || child.kind() == "class" || child.kind() == "module" {
                        // Check if this node defines our identifier
                        let method_name_node = child.child(0)?;
                        let method_name = self.get_node_text(&method_name_node, source_code);

                        if method_name == identifier_text {
                            return Some(self.node_to_range(&method_name_node));
                        }
                    }
                }

                None
            },
            "constant" => {
                // Handle constants
                let constant_text = self.get_node_text(&node, source_code);
                info!("Looking for definition of constant: {}", constant_text);

                // Similar approach as for identifiers
                None
            },
            _ => None,
        }
    }

    // Find definition across workspace files
    fn find_definition_in_workspace(
        &self,
        workspace: &WorkspaceManager,
        current_uri: &Url,
        source_code: &str,
        position: Position
    ) -> Option<Range> {
        // Get the identifier at the current position
        let identifier = self.get_identifier_at_position(source_code, position)?;
        info!("Searching for '{}' across workspace files", identifier);
        
        // Search for the identifier in all indexed files
        for (file_uri, document) in workspace.get_indexed_files() {
            // Skip the current file as we've already searched it
            if file_uri == *current_uri {
                continue;
            }
            
            // Try to parse the document
            if let Some(tree) = RubyParser::new().ok()?.parse(document.get_content()) {
                let root_node = tree.root_node();
                let mut cursor = root_node.walk();
                
                // Look for method/class/module definitions
                for child in root_node.children(&mut cursor) {
                    if child.kind() == "method" || child.kind() == "class" || child.kind() == "module" {
                        // Check if this node defines our identifier
                        if let Some(name_node) = child.child(0) {
                            let name = self.get_node_text(&name_node, document.get_content());
                            
                            if name == identifier {
                                info!("Found definition in file: {}", file_uri);
                                return Some(self.node_to_range(&name_node));
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    // Helper to extract the identifier at a position
    fn get_identifier_at_position(&self, source_code: &str, position: Position) -> Option<String> {
        // Convert position to index in the source code
        let index = self.position_to_index(source_code, position)?;
        
        // Simple approach: extract word at position
        let mut start = index;
        let mut end = index;
        
        // Find start of word
        while start > 0 && source_code.as_bytes()[start - 1].is_ascii_alphanumeric() {
            start -= 1;
        }
        
        // Find end of word
        while end < source_code.len() && source_code.as_bytes()[end].is_ascii_alphanumeric() {
            end += 1;
        }
        
        if start < end {
            Some(source_code[start..end].to_string())
        } else {
            None
        }
    }
    
    // Helper to convert position to index
    fn position_to_index(&self, source_code: &str, position: Position) -> Option<usize> {
        let mut current_line = 0;
        let mut current_character = 0;
        
        for (i, c) in source_code.char_indices() {
            if current_line == position.line as usize && current_character == position.character as usize {
                return Some(i);
            }
            
            if c == '\n' {
                current_line += 1;
                current_character = 0;
            } else {
                current_character += 1;
            }
        }
        
        None
    }

    pub fn get_hover_info(&self, tree: Option<&Tree>, source_code: &str, position: Position) -> Option<String> {
        let tree = tree?;

        // Find the node at the current position
        let node = self.node_at_position(tree, position, source_code)?;

        info!("Hover info requested for node kind: {}", node.kind());

        // Based on the node type, provide appropriate hover information
        match node.kind() {
            "identifier" => {
                let identifier_text = self.get_node_text(&node, source_code);
                let parent = node.parent()?;

                match parent.kind() {
                    "method_call" => {
                        Some(format!("**Method Call**: `{}`\n\nA method call in Ruby.", identifier_text))
                    },
                    "method" => {
                        Some(format!("**Method Definition**: `{}`\n\nA method definition in Ruby.", identifier_text))
                    },
                    "assignment" => {
                        Some(format!("**Variable**: `{}`\n\nA variable in Ruby.", identifier_text))
                    },
                    _ => {
                        Some(format!("**Identifier**: `{}`\n\nAn identifier in Ruby.", identifier_text))
                    }
                }
            },
            "string" => {
                let string_text = self.get_node_text(&node, source_code);
                Some(format!("**String Literal**: `{}`\n\nA string in Ruby.", string_text))
            },
            "integer" => {
                let int_text = self.get_node_text(&node, source_code);
                Some(format!("**Integer Literal**: `{}`\n\nAn integer in Ruby.", int_text))
            },
            "constant" => {
                let const_text = self.get_node_text(&node, source_code);
                Some(format!("**Constant**: `{}`\n\nA constant in Ruby.", const_text))
            },
            "class" => {
                let class_name_node = node.child(0)?;
                let class_name = self.get_node_text(&class_name_node, source_code);
                Some(format!("**Class Definition**: `{}`\n\nA class in Ruby.", class_name))
            },
            "module" => {
                let module_name_node = node.child(0)?;
                let module_name = self.get_node_text(&module_name_node, source_code);
                Some(format!("**Module Definition**: `{}`\n\nA module in Ruby.", module_name))
            },
            _ => {
                // For other node types
                Some(format!("**{}**\n\nA {} in Ruby.", node.kind(), node.kind()))
            }
        }
    }

    pub fn get_completions(&self, tree: Option<&Tree>, source_code: &str, position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // Default Ruby keywords for basic completion
        let default_items = vec![
            CompletionItem {
                label: "def".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a method".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "class".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a class".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "module".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a module".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "if".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Conditional statement".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "else".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Conditional statement".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "elsif".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Conditional statement".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "end".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("End a block".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "do".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Start a block".to_string()),
                ..CompletionItem::default()
            },
        ];

        // Add default items
        items.extend(default_items);

        // If we have a tree, try to add context-specific completions
        if let Some(tree) = tree {
            if let Some(node) = self.node_at_position(tree, position, source_code) {
                info!("Getting completions for node kind: {}", node.kind());

                // Based on the node type and context, provide appropriate completions
                match node.kind() {
                    "class" | "module" => {
                        // Inside a class or module, suggest class/module specific items
                        items.push(CompletionItem {
                            label: "attr_accessor".to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some("Define attribute accessors".to_string()),
                            ..CompletionItem::default()
                        });
                        
                        items.push(CompletionItem {
                            label: "attr_reader".to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some("Define read-only attributes".to_string()),
                            ..CompletionItem::default()
                        });
                        
                        items.push(CompletionItem {
                            label: "attr_writer".to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some("Define write-only attributes".to_string()),
                            ..CompletionItem::default()
                        });
                    },
                    "method" => {
                        // Inside a method, suggest method-specific items
                        items.push(CompletionItem {
                            label: "return".to_string(),
                            kind: Some(CompletionItemKind::KEYWORD),
                            detail: Some("Return a value from a method".to_string()),
                            ..CompletionItem::default()
                        });
                        
                        items.push(CompletionItem {
                            label: "yield".to_string(),
                            kind: Some(CompletionItemKind::KEYWORD),
                            detail: Some("Yield to a block".to_string()),
                            ..CompletionItem::default()
                        });
                    },
                    _ => {
                        // Add more context-specific completions as needed
                    }
                }
            }
        }

        items
    }

    // Helper methods for tree-sitter node manipulation

    fn node_at_position<'a>(&self, tree: &'a Tree, position: Position, _source_code: &str) -> Option<Node<'a>> {
        let point = tree_sitter::Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        
        let node = tree.root_node().named_descendant_for_point_range(point, point)?;
        Some(node)
    }

    fn node_to_range(&self, node: &Node) -> Range {
        let start = Position {
            line: node.start_position().row as u32,
            character: node.start_position().column as u32,
        };
        
        let end = Position {
            line: node.end_position().row as u32,
            character: node.end_position().column as u32,
        };
        
        Range { start, end }
    }

    fn get_node_text(&self, node: &Node, source_code: &str) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        
        source_code[start_byte..end_byte].to_string()
    }

    // This method is currently unused but might be useful in the future
    #[allow(dead_code)]
    fn is_inside_class(&self, _tree: &Tree, node: &Node) -> bool {
        let mut current = Some(*node);
        
        while let Some(n) = current {
            if n.kind() == "class" {
                return true;
            }
            current = n.parent();
        }
        
        false
    }
}
