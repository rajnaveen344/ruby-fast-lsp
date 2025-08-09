# Document Symbols Design Document

## Overview

This design document outlines the implementation of document symbols support for the Ruby Fast LSP, providing hierarchical code outline functionality for Ruby files. The implementation will enable IDE features like outline views, breadcrumb navigation, and quick symbol jumping within a single Ruby file. The design follows LSP protocol standards and integrates seamlessly with the existing Ruby Fast LSP architecture.

## Architecture

### Current Architecture Integration

The document symbols feature will integrate with existing components:
- **Analyzer**: Uses existing Ruby Prism AST parsing and visitor patterns
- **Capabilities**: Follows established capability module structure
- **Server**: Integrates with existing request handling pipeline
- **Types**: Leverages existing Ruby type definitions and document management

### New Components

1. **Document Symbols Capability** (`src/capabilities/document_symbols.rs`)
2. **Document Symbols Visitor** (`src/analyzer_prism/visitors/document_symbols_visitor.rs`)
3. **Symbol Builder** (`src/capabilities/document_symbols/symbol_builder.rs`)
4. **LSP Integration** (Server method and request handler)

## Components and Interfaces

### Core Data Structures

#### DocumentSymbol Structure (LSP Standard)
```rust
// From tower-lsp crate
pub struct DocumentSymbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    pub tags: Option<Vec<SymbolTag>>,
    pub deprecated: Option<bool>,
    pub range: Range,
    pub selection_range: Range,
    pub children: Option<Vec<DocumentSymbol>>,
}
```

#### Ruby Symbol Context
```rust
#[derive(Debug, Clone)]
pub struct RubySymbolContext {
    /// The symbol name
    pub name: String,
    /// Symbol kind (Class, Module, Method, etc.)
    pub kind: SymbolKind,
    /// Additional details (method signature, inheritance, etc.)
    pub detail: Option<String>,
    /// Full range including body
    pub range: Range,
    /// Selection range (just the name)
    pub selection_range: Range,
    /// Nested symbols
    pub children: Vec<RubySymbolContext>,
    /// Symbol visibility (public, private, protected)
    pub visibility: Option<SymbolVisibility>,
    /// Whether it's a class method vs instance method
    pub method_type: Option<MethodType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolVisibility {
    Public,
    Private,
    Protected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MethodType {
    Instance,
    Class,
    Singleton,
}
```

### Document Symbols Visitor

#### Core Visitor Implementation
```rust
use ruby_prism::{Node, Visit};
use tower_lsp::lsp_types::{Position, Range, SymbolKind};
use crate::types::ruby_document::RubyDocument;

pub struct DocumentSymbolsVisitor {
    /// The document being analyzed
    document: RubyDocument,
    /// Stack of current symbol contexts for nesting
    symbol_stack: Vec<RubySymbolContext>,
    /// Final list of top-level symbols
    symbols: Vec<RubySymbolContext>,
    /// Current visibility modifier
    current_visibility: SymbolVisibility,
}

impl DocumentSymbolsVisitor {
    pub fn new(document: RubyDocument) -> Self {
        Self {
            document,
            symbol_stack: Vec::new(),
            symbols: Vec::new(),
            current_visibility: SymbolVisibility::Public,
        }
    }

    pub fn extract_symbols(mut self, root_node: &Node) -> Vec<RubySymbolContext> {
        self.visit(root_node);
        self.symbols
    }

    fn push_symbol(&mut self, symbol: RubySymbolContext) {
        if let Some(parent) = self.symbol_stack.last_mut() {
            parent.children.push(symbol);
        } else {
            self.symbols.push(symbol);
        }
    }

    fn enter_symbol_scope(&mut self, symbol: RubySymbolContext) {
        self.symbol_stack.push(symbol);
    }

    fn exit_symbol_scope(&mut self) {
        if let Some(symbol) = self.symbol_stack.pop() {
            self.push_symbol(symbol);
        }
    }
}
```

#### Visitor Implementation for Ruby Constructs
```rust
impl Visit for DocumentSymbolsVisitor {
    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode) {
        let name = extract_class_name(node);
        let range = node_to_range(&self.document, node);
        let selection_range = name_to_selection_range(&self.document, node);
        
        let detail = extract_class_detail(node); // Inheritance info
        
        let symbol = RubySymbolContext {
            name,
            kind: SymbolKind::CLASS,
            detail,
            range,
            selection_range,
            children: Vec::new(),
            visibility: Some(self.current_visibility.clone()),
            method_type: None,
        };

        self.enter_symbol_scope(symbol);
        
        // Visit class body
        if let Some(body) = node.body() {
            self.visit(&body);
        }
        
        self.exit_symbol_scope();
    }

    fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode) {
        let name = extract_module_name(node);
        let range = node_to_range(&self.document, node);
        let selection_range = name_to_selection_range(&self.document, node);
        
        let symbol = RubySymbolContext {
            name,
            kind: SymbolKind::MODULE,
            detail: None,
            range,
            selection_range,
            children: Vec::new(),
            visibility: Some(self.current_visibility.clone()),
            method_type: None,
        };

        self.enter_symbol_scope(symbol);
        
        // Visit module body
        if let Some(body) = node.body() {
            self.visit(&body);
        }
        
        self.exit_symbol_scope();
    }

    fn visit_def_node(&mut self, node: &ruby_prism::DefNode) {
        let name = extract_method_name(node);
        let range = node_to_range(&self.document, node);
        let selection_range = name_to_selection_range(&self.document, node);
        
        let (detail, method_type) = extract_method_details(node);
        
        let symbol = RubySymbolContext {
            name,
            kind: SymbolKind::METHOD,
            detail: Some(detail),
            range,
            selection_range,
            children: Vec::new(),
            visibility: Some(self.current_visibility.clone()),
            method_type: Some(method_type),
        };

        self.push_symbol(symbol);
    }

    fn visit_constant_write_node(&mut self, node: &ruby_prism::ConstantWriteNode) {
        let name = node.name().to_string();
        let range = node_to_range(&self.document, node);
        let selection_range = name_to_selection_range(&self.document, node);
        
        let symbol = RubySymbolContext {
            name,
            kind: SymbolKind::CONSTANT,
            detail: None,
            range,
            selection_range,
            children: Vec::new(),
            visibility: Some(self.current_visibility.clone()),
            method_type: None,
        };

        self.push_symbol(symbol);
    }

    fn visit_call_node(&mut self, node: &ruby_prism::CallNode) {
        // Handle visibility modifiers and attr_* methods
        if let Some(method_name) = node.name() {
            match method_name {
                "private" => self.current_visibility = SymbolVisibility::Private,
                "protected" => self.current_visibility = SymbolVisibility::Protected,
                "public" => self.current_visibility = SymbolVisibility::Public,
                "attr_reader" | "attr_writer" | "attr_accessor" => {
                    self.handle_attr_methods(node);
                }
                _ => {}
            }
        }

        // Continue visiting children
        ruby_prism::visit_child_nodes(self, node);
    }
}
```

### Symbol Builder Utilities

#### Range and Position Utilities
```rust
pub fn node_to_range(document: &RubyDocument, node: &Node) -> Range {
    let start_offset = node.location().start_offset();
    let end_offset = node.location().end_offset();
    
    Range {
        start: document.offset_to_position(start_offset),
        end: document.offset_to_position(end_offset),
    }
}

pub fn name_to_selection_range(document: &RubyDocument, node: &Node) -> Range {
    // Extract just the name portion of the node
    // This is more complex and depends on node type
    match node {
        Node::ClassNode(class_node) => {
            let name_location = class_node.constant_path().location();
            Range {
                start: document.offset_to_position(name_location.start_offset()),
                end: document.offset_to_position(name_location.end_offset()),
            }
        }
        // ... other node types
        _ => node_to_range(document, node), // Fallback
    }
}
```

#### Detail Extraction
```rust
pub fn extract_class_detail(node: &ruby_prism::ClassNode) -> Option<String> {
    if let Some(superclass) = node.superclass() {
        Some(format!("< {}", extract_constant_path(&superclass)))
    } else {
        None
    }
}

pub fn extract_method_details(node: &ruby_prism::DefNode) -> (String, MethodType) {
    let mut detail = String::new();
    let method_type = if node.receiver().is_some() {
        MethodType::Class
    } else {
        MethodType::Instance
    };

    // Build parameter list
    if let Some(parameters) = node.parameters() {
        detail.push('(');
        // Extract parameter details...
        detail.push(')');
    }

    (detail, method_type)
}

pub fn handle_attr_methods(&mut self, node: &ruby_prism::CallNode) {
    // Extract attr_reader, attr_writer, attr_accessor symbols
    if let Some(arguments) = node.arguments() {
        for arg in arguments.arguments() {
            if let Node::SymbolNode(symbol_node) = arg {
                let name = extract_symbol_value(symbol_node);
                let range = node_to_range(&self.document, arg);
                let selection_range = range.clone();
                
                let symbol = RubySymbolContext {
                    name,
                    kind: SymbolKind::PROPERTY,
                    detail: Some("attr accessor".to_string()),
                    range,
                    selection_range,
                    children: Vec::new(),
                    visibility: Some(self.current_visibility.clone()),
                    method_type: Some(MethodType::Instance),
                };

                self.push_symbol(symbol);
            }
        }
    }
}
```

### Capability Implementation

#### Main Document Symbols Handler
```rust
// src/capabilities/document_symbols.rs
use tower_lsp::lsp_types::{DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, SymbolKind, Url};
use crate::server::RubyLanguageServer;
use crate::analyzer_prism::visitors::document_symbols_visitor::DocumentSymbolsVisitor;

pub async fn handle_document_symbols(
    server: &RubyLanguageServer,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;
    
    // Get document content
    let document = server.get_doc(&uri)?;
    
    // Parse Ruby code
    let parse_result = ruby_prism::parse(document.content.as_bytes());
    let root_node = parse_result.node();
    
    // Extract symbols using visitor
    let visitor = DocumentSymbolsVisitor::new(document);
    let ruby_symbols = visitor.extract_symbols(&root_node);
    
    // Convert to LSP DocumentSymbol format
    let lsp_symbols = ruby_symbols
        .into_iter()
        .map(|symbol| convert_to_document_symbol(symbol))
        .collect();
    
    Some(DocumentSymbolResponse::Nested(lsp_symbols))
}

fn convert_to_document_symbol(ruby_symbol: RubySymbolContext) -> DocumentSymbol {
    DocumentSymbol {
        name: ruby_symbol.name,
        detail: ruby_symbol.detail,
        kind: ruby_symbol.kind,
        tags: None,
        deprecated: None,
        range: ruby_symbol.range,
        selection_range: ruby_symbol.selection_range,
        children: if ruby_symbol.children.is_empty() {
            None
        } else {
            Some(
                ruby_symbol
                    .children
                    .into_iter()
                    .map(convert_to_document_symbol)
                    .collect(),
            )
        },
    }
}
```

### Server Integration

#### LSP Server Method Addition
```rust
// In src/server.rs - add to LanguageServer impl
async fn document_symbol(
    &self,
    params: DocumentSymbolParams,
) -> LspResult<Option<DocumentSymbolResponse>> {
    info!(
        "Document symbol request received for {:?}",
        params.text_document.uri.path()
    );
    
    let start_time = Instant::now();
    let result = request::handle_document_symbols(self, params).await;
    
    info!(
        "[PERF] Document symbols completed in {:?}",
        start_time.elapsed()
    );
    
    Ok(result)
}
```

#### Request Handler Addition
```rust
// In src/handlers/request.rs
pub async fn handle_document_symbols(
    lang_server: &RubyLanguageServer,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    document_symbols::handle_document_symbols(lang_server, params).await
}
```

#### Capability Registration
```rust
// In src/handlers/notification.rs - update initialize handler
pub fn get_server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        // ... existing capabilities
        document_symbol_provider: Some(OneOf::Left(true)),
        // ... rest of capabilities
    }
}
```

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1)
1. **Create capability module structure**
   - `src/capabilities/document_symbols.rs`
   - `src/capabilities/document_symbols/mod.rs`
   - `src/capabilities/document_symbols/symbol_builder.rs`

2. **Basic visitor implementation**
   - `src/analyzer_prism/visitors/document_symbols_visitor.rs`
   - Support for classes, modules, methods, constants

3. **LSP integration**
   - Server method implementation
   - Request handler setup
   - Capability registration

### Phase 2: Ruby Constructs Support (Week 2)
1. **Advanced method handling**
   - Class methods vs instance methods
   - Method parameters and signatures
   - Visibility modifiers (private, protected, public)

2. **Attribute methods**
   - `attr_reader`, `attr_writer`, `attr_accessor`
   - Dynamic attribute generation

3. **Nested structures**
   - Proper parent-child relationships
   - Scope tracking and nesting

### Phase 3: Advanced Features (Week 3)
1. **Metaprogramming support**
   - `define_method` detection
   - Dynamic constant definitions
   - Singleton methods

2. **Range accuracy**
   - Precise selection ranges
   - Proper body range calculation
   - Edge case handling

3. **Performance optimization**
   - Caching strategies
   - Incremental updates
   - Memory efficiency

### Phase 4: Testing and Polish (Week 4)
1. **Comprehensive testing**
   - Unit tests for visitor
   - Integration tests for LSP
   - Snapshot testing for symbol output

2. **Error handling**
   - Malformed syntax handling
   - Graceful degradation
   - Performance monitoring

3. **Documentation and examples**
   - API documentation
   - Usage examples
   - Performance benchmarks

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ruby_document::RubyDocument;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn test_class_symbol_extraction() {
        let content = r#"
class MyClass < BaseClass
  def instance_method
  end

  def self.class_method
  end
end
        "#;
        
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 0);
        let parse_result = ruby_prism::parse(content.as_bytes());
        
        let visitor = DocumentSymbolsVisitor::new(document);
        let symbols = visitor.extract_symbols(&parse_result.node());
        
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "MyClass");
        assert_eq!(symbols[0].kind, SymbolKind::CLASS);
        assert_eq!(symbols[0].detail, Some("< BaseClass".to_string()));
        assert_eq!(symbols[0].children.len(), 2);
    }

    #[test]
    fn test_nested_modules() {
        let content = r#"
module Outer
  module Inner
    CONSTANT = 42
    
    def helper_method
    end
  end
end
        "#;
        
        // Test nested structure...
    }

    #[test]
    fn test_attr_methods() {
        let content = r#"
class User
  attr_reader :name, :email
  attr_accessor :age
end
        "#;
        
        // Test attribute method symbols...
    }
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_document_symbols_integration() {
    let server = RubyLanguageServer::default();
    let uri = Url::parse("file:///test.rb").unwrap();
    
    // Setup document...
    
    let params = DocumentSymbolParams {
        text_document: TextDocumentIdentifier { uri },
    };
    
    let result = handle_document_symbols(&server, params).await;
    
    assert!(result.is_some());
    // Verify symbol structure...
}
```

### Performance Tests
```rust
#[test]
fn test_large_file_performance() {
    // Generate large Ruby file with many symbols
    let large_content = generate_large_ruby_file(1000); // 1000 classes
    
    let start = Instant::now();
    let symbols = extract_document_symbols(&large_content);
    let duration = start.elapsed();
    
    assert!(duration < Duration::from_millis(100));
    assert_eq!(symbols.len(), 1000);
}
```

## Error Handling Strategy

### Graceful Degradation
1. **Syntax Errors**: Return symbols for parseable portions
2. **Unknown Constructs**: Skip gracefully, continue processing
3. **Performance Issues**: Implement timeouts and limits
4. **Memory Constraints**: Use streaming and chunking for large files

### Error Recovery
```rust
impl DocumentSymbolsVisitor {
    fn handle_parse_error(&mut self, node: &Node, error: &str) {
        log::warn!("Failed to process node: {}", error);
        // Continue with partial symbol information
    }
    
    fn with_timeout<F, R>(&self, operation: F) -> Option<R>
    where
        F: FnOnce() -> R,
    {
        // Implement timeout for expensive operations
        Some(operation())
    }
}
```

## Performance Considerations

### Optimization Strategies
1. **Lazy Evaluation**: Only compute symbols when requested
2. **Incremental Updates**: Update only changed portions
3. **Caching**: Cache symbol trees for unchanged files
4. **Memory Management**: Use efficient data structures

### Performance Targets
- **Small files (<100 lines)**: <10ms
- **Medium files (100-1000 lines)**: <50ms
- **Large files (1000+ lines)**: <100ms
- **Memory usage**: <5MB per file

## Future Enhancements

### Potential Extensions
1. **Workspace Symbols**: Cross-file symbol search
2. **Symbol Filtering**: Filter by type, visibility, etc.
3. **Symbol Hierarchy**: Show inheritance relationships
4. **Documentation Integration**: Include RDoc/YARD comments
5. **Type Information**: Show inferred types where available

### Integration Opportunities
1. **Semantic Tokens**: Coordinate with syntax highlighting
2. **Go-to-Definition**: Use symbols for navigation
3. **References**: Include symbol outline in reference search
4. **Completion**: Use symbols for context-aware completion

This design provides a comprehensive foundation for implementing document symbols in the Ruby Fast LSP, following established patterns while providing robust Ruby-specific functionality.