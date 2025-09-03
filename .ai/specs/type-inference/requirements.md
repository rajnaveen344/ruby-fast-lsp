# Type Inference Requirements Document

## Introduction

This specification defines comprehensive type inference support for the Ruby Fast LSP, enabling intelligent type analysis and context-aware completions based on inferred types. The type inference system will analyze Ruby code to determine variable types, method return types, and provide union type support for Ruby's dynamic nature. This feature will significantly enhance developer productivity by providing accurate completions, better error detection, and improved code understanding.

## Requirements

### Requirement 1: Basic Type Inference

**User Story:** As a Ruby developer, I want the LSP to infer types from literal assignments and method calls so that I can get accurate completions and type information without explicit type annotations.

#### Acceptance Criteria

1. WHEN I assign a string literal THEN the system SHALL infer `String` type
2. WHEN I assign a numeric literal THEN the system SHALL infer `Integer` or `Float` type
3. WHEN I assign a boolean literal THEN the system SHALL infer `TrueClass` or `FalseClass` type
4. WHEN I assign `nil` THEN the system SHALL infer `NilClass` type
5. WHEN I assign an array literal THEN the system SHALL infer `Array` type with element types
6. WHEN I assign a hash literal THEN the system SHALL infer `Hash` type with key and value types
7. WHEN I assign a symbol literal THEN the system SHALL infer `Symbol` type
8. WHEN I assign a class reference THEN the system SHALL infer class reference type

### Requirement 2: Union Type Support

**User Story:** As a Ruby developer working with dynamic code, I want the type system to handle multiple possible types for variables so that completions remain accurate across different code paths.

#### Acceptance Criteria

1. WHEN a variable can have multiple types THEN the system SHALL create union types
2. WHEN I assign different types in conditional branches THEN the system SHALL merge types into unions
3. WHEN I call methods on union types THEN the system SHALL provide completions for all possible types
4. WHEN polymorphic arrays are created THEN the system SHALL track multiple element types
5. WHEN polymorphic hashes are created THEN the system SHALL track multiple key and value types
6. WHEN type narrowing occurs THEN the system SHALL refine union types appropriately

### Requirement 3: Method Return Type Inference

**User Story:** As a Ruby developer, I want the system to infer method return types from method bodies so that method calls provide accurate type information for completions.

#### Acceptance Criteria

1. WHEN a method has explicit return statements THEN the system SHALL infer return type from all return paths
2. WHEN a method has implicit returns THEN the system SHALL infer type from the last expression
3. WHEN a method has multiple return types THEN the system SHALL create union return types
4. WHEN a method calls other methods THEN the system SHALL use their return types for inference
5. WHEN a method has no explicit returns THEN the system SHALL infer `NilClass` type
6. WHEN recursive method calls occur THEN the system SHALL handle them without infinite loops

### Requirement 4: Signature-First Type Inference

**User Story:** As a Ruby developer using type annotations, I want the system to prioritize explicit type signatures over inferred types so that I can have authoritative type information.

#### Acceptance Criteria

1. WHEN RBS signatures are present THEN they SHALL take precedence over inferred types
2. WHEN Sorbet signatures are present THEN they SHALL take precedence over inferred types
3. WHEN YARD type annotations are present THEN they SHALL be used for type information
4. WHEN method signatures define parameter types THEN call sites SHALL be validated against them
5. WHEN method signatures define return types THEN they SHALL be used instead of inference
6. WHEN signature parsing fails THEN the system SHALL fall back to structural inference

### Requirement 5: Flow-Sensitive Type Analysis

**User Story:** As a Ruby developer using conditionals and type checks, I want the type system to understand control flow so that type information is accurate within different code branches.

#### Acceptance Criteria

1. WHEN `is_a?` checks are used THEN types SHALL be narrowed in the true branch
2. WHEN `kind_of?` checks are used THEN types SHALL be narrowed appropriately
3. WHEN `nil?` checks are used THEN `NilClass` SHALL be excluded from union types in false branch
4. WHEN case statements are used THEN types SHALL be narrowed based on case conditions
5. WHEN guard clauses are used THEN types SHALL be refined after the guard
6. WHEN rescue blocks are used THEN exception types SHALL be inferred for the rescue variable

### Requirement 6: Performance and Incremental Analysis

**User Story:** As a developer working on large Ruby projects, I want type inference to be fast and responsive so that it doesn't impact LSP performance.

#### Acceptance Criteria

1. WHEN files are modified THEN only affected types SHALL be re-inferred
2. WHEN type inference runs THEN it SHALL complete within 100ms for typical files
3. WHEN large files are analyzed THEN memory usage SHALL remain reasonable
4. WHEN type caching is used THEN cache invalidation SHALL be accurate
5. WHEN circular dependencies exist THEN the system SHALL handle them gracefully
6. WHEN type inference fails THEN it SHALL not crash the LSP server

### Requirement 7: Integration with Existing Features

**User Story:** As a Ruby developer, I want type inference to enhance existing LSP features like completion, hover, and diagnostics without breaking current functionality.

#### Acceptance Criteria

1. WHEN completions are requested THEN type information SHALL improve suggestion accuracy
2. WHEN hover information is requested THEN inferred types SHALL be displayed
3. WHEN method completions are shown THEN they SHALL be filtered by receiver type
4. WHEN constant completions are shown THEN class/module references SHALL be properly typed
5. WHEN existing completion features work THEN type inference SHALL not break them
6. WHEN type information is unavailable THEN the system SHALL gracefully fall back to existing behavior

## Non-Functional Requirements

### Performance Requirements
- Type inference SHALL complete within 100ms for files under 1000 lines
- Memory usage SHALL not exceed 50MB for type information storage
- Incremental updates SHALL process within 50ms for typical changes

### Reliability Requirements
- Type inference errors SHALL not crash the LSP server
- Fallback mechanisms SHALL ensure LSP functionality continues without types
- Type cache corruption SHALL be detected and recovered automatically

### Compatibility Requirements
- Integration SHALL maintain compatibility with existing RubyIndex infrastructure
- Type system SHALL work with Ruby versions 1.9 through 3.4
- Signature parsing SHALL support RBS, Sorbet, and YARD formats

## Success Criteria

1. **Accuracy**: Type inference achieves >85% accuracy on common Ruby patterns
2. **Performance**: No noticeable impact on LSP responsiveness
3. **Coverage**: Type information available for >70% of variables and method calls
4. **Integration**: Seamless enhancement of existing completion and hover features
5. **Robustness**: Graceful handling of edge cases and malformed code