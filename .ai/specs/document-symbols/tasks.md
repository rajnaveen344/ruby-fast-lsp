# Document Symbols Task List

## ✅ Completed Tasks

### Core Implementation
- [x] **DocumentSymbolsVisitor Structure** - Created basic visitor structure with proper lifetime management
- [x] **RubyDocument Integration** - Updated visitor to use existing RubyDocument utilities for position/offset conversions
- [x] **Basic Symbol Creation** - Implemented `create_symbol` helper function using `prism_location_to_lsp_range`
- [x] **Visit Trait Implementation** - Fixed Visit trait with correct lifetime syntax `Visit<'a>`
- [x] **RubySymbolContext Structure** - Updated structure with all required fields (name, kind, detail, range, selection_range, children, visibility, method_type)
- [x] **Compilation Fixes** - Resolved all compilation errors and type mismatches
- [x] **Document Symbols Handler** - Updated `convert_to_document_symbol` to work with nested structure

### Basic Symbol Support
- [x] **Class Symbols** - Basic class symbol extraction with `SymbolKind::CLASS`
- [x] **Module Symbols** - Basic module symbol extraction with `SymbolKind::MODULE`
- [x] **Method Symbols** - Basic method symbol extraction with `SymbolKind::METHOD`
- [x] **Constant Symbols** - Basic constant symbol extraction with `SymbolKind::CONSTANT`

## 🚧 In Progress Tasks

### Symbol Enhancement
- [ ] **Method Details** - Extract method parameters and signatures for symbol details
- [ ] **Class Inheritance** - Extract and display class inheritance information
- [ ] **Visibility Tracking** - Implement proper visibility modifier tracking (private, protected, public)
- [ ] **Method Type Detection** - Distinguish between instance methods, class methods, and singleton methods

## 📋 Pending Tasks

### Advanced Ruby Constructs
- [ ] **Attribute Methods** - Handle `attr_reader`, `attr_writer`, `attr_accessor` as symbols
- [ ] **Singleton Classes** - Support singleton class definitions (`class << self`)
- [ ] **Dynamic Methods** - Handle `define_method` and other metaprogramming constructs
- [ ] **Alias Methods** - Support method aliases and their relationships
- [ ] **Block Parameters** - Include block parameters in method signatures

### Symbol Hierarchy & Nesting
- [ ] **Nested Classes** - Ensure proper nesting of classes within modules/classes
- [ ] **Nested Modules** - Handle deeply nested module structures
- [ ] **Namespace Constants** - Properly nest constants within their containing namespaces
- [ ] **Scope Tracking** - Implement proper scope tracking for nested symbols

### Symbol Details & Metadata
- [ ] **Parameter Lists** - Extract and format method parameter information
- [ ] **Default Parameters** - Show default parameter values in method signatures
- [ ] **Keyword Arguments** - Handle keyword arguments in method signatures
- [ ] **Return Types** - Extract return type information where available (comments, YARD)
- [ ] **Documentation** - Extract and include documentation strings

### Performance & Optimization
- [ ] **Incremental Updates** - Implement incremental symbol updates on file changes
- [ ] **Caching Strategy** - Add caching for symbol information
- [ ] **Large File Handling** - Optimize for files with many symbols
- [ ] **Memory Management** - Implement efficient memory usage for symbol storage

### Error Handling & Edge Cases
- [ ] **Syntax Error Handling** - Handle files with syntax errors gracefully
- [ ] **Malformed Code** - Skip malformed constructs without crashing
- [ ] **Encoding Issues** - Handle various file encodings properly
- [ ] **Complex Metaprogramming** - Gracefully handle complex metaprogramming patterns

### Testing & Validation
- [x] **Unit Tests** - Create comprehensive unit tests for all symbol types
- [ ] **Integration Tests** - Test end-to-end symbol generation
- [ ] **Performance Tests** - Benchmark symbol generation performance
- [ ] **Edge Case Tests** - Test various edge cases and malformed code
- [ ] **Snapshot Tests** - Create snapshot tests for symbol output validation

### LSP Integration
- [ ] **Symbol Filtering** - Implement symbol filtering by kind and visibility
- [ ] **Symbol Search** - Add symbol search capabilities
- [ ] **Navigation Integration** - Ensure proper integration with go-to-definition
- [ ] **Reference Integration** - Connect with reference finding functionality
- [ ] **Workspace Symbols** - Consider integration with workspace-wide symbol search

### Documentation & Polish
- [ ] **API Documentation** - Document all public APIs and structures
- [ ] **Usage Examples** - Create examples of symbol usage
- [ ] **Performance Guidelines** - Document performance characteristics
- [ ] **Troubleshooting Guide** - Create troubleshooting documentation

## 🎯 Priority Tasks (Next Sprint)

1. **Method Details Enhancement** - Extract method parameters and improve symbol details
2. **Visibility Tracking** - Implement proper visibility modifier tracking
3. **Attribute Methods** - Handle attr_* methods as symbols
4. **Unit Testing** - Create comprehensive test suite
5. **Performance Optimization** - Optimize for large files

## 📝 Notes

- Current implementation provides basic symbol extraction for classes, modules, methods, and constants
- All compilation errors have been resolved and the feature builds successfully
- The visitor uses existing RubyDocument utilities for position conversions
- Symbol hierarchy is implemented with direct nesting (children stored as Vec<RubySymbolContext>)
- Ready for enhancement with more detailed symbol information and advanced Ruby constructs

## 🐛 Known Issues

- [ ] **Deprecated Field Warning** - `DocumentSymbol.deprecated` field shows deprecation warning (use tags instead)
- [ ] **Unused Method Warning** - `extract_node_name` method is currently unused
- [ ] **Limited Method Details** - Method signatures are not yet extracted
- [ ] **No Visibility Tracking** - Visibility modifiers are not properly tracked across scopes

## 🔄 Recent Changes

- **2024-01-XX**: Fixed compilation errors and updated to use RubyDocument utilities
- **2024-01-XX**: Implemented basic symbol extraction for core Ruby constructs
- **2024-01-XX**: Updated symbol structure to support direct nesting
- **2024-01-XX**: Added comprehensive test suite with 15 tests covering:
  - Basic symbol extraction (classes, modules, methods, constants)
  - Nested structures and inheritance
  - Visibility modifiers (private, protected, public)
  - Method parameters and types (instance vs class methods)
  - Edge cases (empty files, comments, singleton classes)
  - Symbol ranges and positioning
  - Complex Ruby constructs
- **2024-01-XX**: Fixed child node traversal to properly extract nested symbols
- **2024-01-XX**: All tests passing successfully