# Document Symbols Requirements Document

## Introduction

This specification defines comprehensive document symbols support for the Ruby Fast LSP, enabling code outline functionality and hierarchical navigation within Ruby files. Document symbols provide a structured view of a Ruby file's contents, showing classes, modules, methods, constants, and other significant code elements in a tree-like structure. This feature is essential for code navigation, understanding file structure, and providing IDE features like outline views and breadcrumb navigation.

## Requirements

### Requirement 1: Basic Symbol Hierarchy

**User Story:** As a Ruby developer using the LSP, I want to see a hierarchical outline of my Ruby file showing classes, modules, methods, and constants so that I can quickly navigate and understand the file structure.

#### Acceptance Criteria

1. WHEN a Ruby file contains top-level classes THEN the system SHALL create `DocumentSymbol` entries with `SymbolKind::Class`
2. WHEN a Ruby file contains top-level modules THEN the system SHALL create `DocumentSymbol` entries with `SymbolKind::Module`
3. WHEN a Ruby file contains methods THEN the system SHALL create `DocumentSymbol` entries with `SymbolKind::Method`
4. WHEN a Ruby file contains constants THEN the system SHALL create `DocumentSymbol` entries with `SymbolKind::Constant`
5. WHEN symbols are nested THEN the system SHALL create proper parent-child relationships in the symbol hierarchy
6. WHEN symbols have ranges THEN the system SHALL provide accurate `range` (full symbol including body) and `selectionRange` (just the name)

### Requirement 2: Nested Symbol Structure

**User Story:** As a Ruby developer, I want the document symbols to accurately reflect the nested structure of my Ruby code so that I can understand the relationships between classes, modules, and methods.

#### Acceptance Criteria

1. WHEN a class contains methods THEN the methods SHALL be children of the class symbol
2. WHEN a module contains classes THEN the classes SHALL be children of the module symbol
3. WHEN a class inherits from another class THEN the inheritance SHALL be reflected in the symbol detail
4. WHEN modules are nested within other modules THEN the nesting SHALL be accurately represented
5. WHEN methods are defined within singleton classes THEN they SHALL be properly nested under the singleton class symbol
6. WHEN constants are defined within namespaces THEN they SHALL be children of their containing namespace

### Requirement 3: Method Symbol Details

**User Story:** As a Ruby developer, I want detailed information about methods in the document symbols so that I can understand method signatures, visibility, and characteristics.

#### Acceptance Criteria

1. WHEN a method has parameters THEN the symbol detail SHALL include the parameter list
2. WHEN a method is private, protected, or public THEN the visibility SHALL be indicated in the symbol
3. WHEN a method is a class method (self.method) THEN it SHALL be distinguished from instance methods
4. WHEN a method has a block parameter THEN it SHALL be included in the method signature
5. WHEN a method has default parameters THEN they SHALL be shown in the symbol detail
6. WHEN a method is an alias THEN both the original and alias SHALL be represented appropriately

### Requirement 4: Advanced Ruby Constructs

**User Story:** As a Ruby developer working with advanced Ruby features, I want the document symbols to handle metaprogramming, dynamic definitions, and Ruby-specific constructs accurately.

#### Acceptance Criteria

1. WHEN methods are defined using `define_method` THEN they SHALL be included in the symbols
2. WHEN attributes are defined using `attr_reader`, `attr_writer`, `attr_accessor` THEN they SHALL be represented as appropriate symbols
3. WHEN constants are defined dynamically THEN they SHALL be captured where statically analyzable
4. WHEN singleton methods are defined THEN they SHALL be properly categorized and nested
5. WHEN modules are included/extended/prepended THEN the mixin relationships SHALL be indicated in symbol details
6. WHEN blocks define local scope THEN significant block constructs SHALL be represented appropriately

### Requirement 5: Symbol Positioning and Ranges

**User Story:** As a Ruby developer, I want accurate positioning information for document symbols so that clicking on symbols in the outline navigates to the correct location in the code.

#### Acceptance Criteria

1. WHEN a symbol is created THEN the `range` SHALL encompass the entire symbol definition including its body
2. WHEN a symbol is created THEN the `selectionRange` SHALL cover only the symbol name for precise navigation
3. WHEN symbols span multiple lines THEN the ranges SHALL accurately reflect the start and end positions
4. WHEN symbols are nested THEN child symbol ranges SHALL be contained within parent symbol ranges
5. WHEN symbols have complex definitions THEN the ranges SHALL handle edge cases like heredocs and string interpolation
6. WHEN the cursor is positioned on a symbol THEN the corresponding outline entry SHALL be highlighted

### Requirement 6: Symbol Filtering and Categorization

**User Story:** As a Ruby developer, I want the ability to filter and categorize document symbols so that I can focus on specific types of code elements.

#### Acceptance Criteria

1. WHEN requesting document symbols THEN the system SHALL support filtering by symbol kind
2. WHEN symbols are returned THEN they SHALL be categorized using appropriate LSP `SymbolKind` values
3. WHEN symbols have different visibility levels THEN the system SHALL provide mechanisms to filter by visibility
4. WHEN symbols are deprecated or have special annotations THEN this SHALL be reflected in the symbol information
5. WHEN the file contains many symbols THEN the system SHALL provide efficient filtering capabilities
6. WHEN symbols are dynamically generated THEN they SHALL be appropriately categorized

### Requirement 7: Performance and Scalability

**User Story:** As a developer working on large Ruby files, I want document symbols to be generated quickly without impacting editor responsiveness.

#### Acceptance Criteria

1. WHEN generating document symbols THEN the operation SHALL complete within 100ms for typical Ruby files
2. WHEN processing large files (>1000 lines) THEN symbol generation SHALL not block the editor
3. WHEN files are modified THEN symbol updates SHALL be incremental where possible
4. WHEN multiple files request symbols simultaneously THEN the system SHALL handle concurrent requests efficiently
5. WHEN memory usage grows THEN the system SHALL implement appropriate caching strategies
6. WHEN symbol generation fails THEN the system SHALL degrade gracefully without crashing

### Requirement 8: Integration with Existing Features

**User Story:** As a Ruby developer, I want document symbols to integrate seamlessly with other LSP features like go-to-definition, references, and semantic highlighting.

#### Acceptance Criteria

1. WHEN clicking on a symbol in the outline THEN it SHALL navigate to the symbol definition
2. WHEN symbols are renamed THEN the document symbols SHALL be updated accordingly
3. WHEN using go-to-definition THEN it SHALL work from symbol outline entries
4. WHEN finding references THEN symbols in the outline SHALL be included in reference results
5. WHEN semantic tokens are updated THEN document symbols SHALL remain consistent
6. WHEN the indexer updates symbol information THEN document symbols SHALL reflect the changes

### Requirement 9: Error Handling and Edge Cases

**User Story:** As a Ruby developer, I want the document symbols feature to handle malformed code and edge cases gracefully without breaking the outline functionality.

#### Acceptance Criteria

1. WHEN the Ruby file has syntax errors THEN the system SHALL provide symbols for the parseable portions
2. WHEN encountering unknown or complex metaprogramming THEN the system SHALL skip gracefully
3. WHEN symbol definitions are incomplete THEN the system SHALL provide partial symbol information
4. WHEN files are very large THEN the system SHALL implement appropriate limits and warnings
5. WHEN encoding issues exist THEN the system SHALL handle them without crashing
6. WHEN circular dependencies exist in symbol definitions THEN the system SHALL detect and handle them

### Requirement 10: Testing and Validation

**User Story:** As a maintainer of the Ruby Fast LSP, I want comprehensive tests for document symbols functionality to ensure reliability and prevent regressions.

#### Acceptance Criteria

1. WHEN document symbols functionality is implemented THEN unit tests SHALL cover all symbol types
2. WHEN complex Ruby files exist THEN integration tests SHALL verify end-to-end symbol generation
3. WHEN edge cases are identified THEN specific test cases SHALL be added
4. WHEN performance requirements exist THEN benchmark tests SHALL validate performance
5. WHEN the implementation changes THEN existing tests SHALL continue to pass
6. WHEN snapshot testing is used THEN symbol output SHALL be validated against expected structures

## Non-Functional Requirements

### Performance
- Document symbol generation SHALL complete within 100ms for files up to 1000 lines
- Memory usage for symbol storage SHALL not exceed 5% of total LSP memory per file
- Symbol updates SHALL be incremental and complete within 50ms for typical changes

### Reliability
- The system SHALL handle malformed Ruby syntax without crashing
- Symbol generation SHALL not interfere with other LSP operations
- The system SHALL maintain consistency between symbols and actual code structure

### Usability
- Symbol names SHALL be clear and descriptive
- Symbol hierarchy SHALL accurately reflect Ruby's scoping rules
- Symbol details SHALL provide useful information for navigation and understanding

### Maintainability
- Document symbols logic SHALL be modular and testable
- The implementation SHALL follow existing code patterns and conventions
- Symbol generation SHALL be extensible for future Ruby language features

## Success Criteria

The document symbols support will be considered successful when:

1. **Accuracy**: 95% of Ruby symbols are correctly identified and positioned
2. **Performance**: Symbol generation adds less than 100ms to file opening time
3. **Coverage**: All major Ruby constructs (classes, modules, methods, constants) are supported
4. **Reliability**: No crashes or incorrect symbols in production usage
5. **User Experience**: Developers can navigate Ruby files efficiently using the outline view
6. **Integration**: Seamless integration with existing LSP features and VS Code outline functionality

## Implementation Notes

### LSP Protocol Compliance
- Use `textDocument/documentSymbol` request as defined in LSP specification
- Return `DocumentSymbol[]` with proper hierarchy rather than flat `SymbolInformation[]`
- Support both `DocumentSymbol` and `SymbolInformation` for client compatibility

### Ruby-Specific Considerations
- Handle Ruby's flexible syntax and metaprogramming features
- Respect Ruby's scoping and visibility rules
- Support both traditional and modern Ruby syntax patterns
- Consider Rails and common gem patterns where applicable

### Future Extensibility
- Design symbol extraction to be extensible for new Ruby features
- Consider integration with workspace symbols for cross-file navigation
- Plan for potential integration with type information and documentation