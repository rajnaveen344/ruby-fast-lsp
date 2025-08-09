# Constant Completion Requirements Document

## Introduction

This specification defines comprehensive auto completion support for Ruby constants (classes and modules) in the Ruby Fast LSP. This feature will enable intelligent completion of class names, module names, and other constants across the workspace, leveraging the existing RubyIndex infrastructure to provide fast, accurate, and context-aware suggestions. The implementation will enhance developer productivity by providing instant access to available constants without requiring manual navigation or memorization of class/module names.

## Requirements

### Requirement 1: Basic Constant Completion

**User Story:** As a Ruby developer, I want auto completion for class and module names so that I can quickly reference constants without typing their full names or remembering exact spelling.

#### Acceptance Criteria

1. WHEN I type a partial class name THEN the system SHALL suggest matching class definitions from the workspace
2. WHEN I type a partial module name THEN the system SHALL suggest matching module definitions from the workspace
3. WHEN I type a partial constant name THEN the system SHALL suggest matching constant definitions from the workspace
4. WHEN multiple constants match THEN the system SHALL rank them by relevance and proximity
5. WHEN I select a completion THEN the system SHALL insert the correct constant name
6. WHEN constants are in different namespaces THEN the system SHALL show the fully qualified name when necessary

### Requirement 2: Context-Aware Completion

**User Story:** As a Ruby developer, I want constant completion to be context-aware so that it suggests the most relevant constants based on my current location in the code.

#### Acceptance Criteria

1. WHEN I'm inside a namespace THEN the system SHALL prioritize constants within the same namespace
2. WHEN I'm in a class/module THEN the system SHALL suggest constants accessible from that scope
3. WHEN I type after `::` THEN the system SHALL suggest constants within the specified namespace
4. WHEN I'm in a method THEN the system SHALL consider the method's context for constant resolution
5. WHEN constants are imported via `include`/`extend` THEN they SHALL be available for completion
6. WHEN constants have visibility modifiers THEN only accessible constants SHALL be suggested

### Requirement 3: Namespace-Aware Completion

**User Story:** As a Ruby developer working with namespaced code, I want completion to understand Ruby's constant lookup rules so that I can efficiently navigate complex namespace hierarchies.

#### Acceptance Criteria

1. WHEN I type a qualified constant name (e.g., `Foo::Bar`) THEN the system SHALL complete within that namespace
2. WHEN I use relative constant references THEN the system SHALL resolve them according to Ruby's lookup rules
3. WHEN constants are nested deeply THEN the system SHALL provide completion at each level
4. WHEN I'm in a nested namespace THEN the system SHALL suggest both local and parent namespace constants
5. WHEN constants are aliased THEN both original and aliased names SHALL be available
6. WHEN autoloading is used THEN the system SHALL suggest constants that would be autoloaded

### Requirement 4: Performance and Scalability

**User Story:** As a developer working on large Ruby projects, I want constant completion to be fast and responsive so that it doesn't interrupt my coding flow.

#### Acceptance Criteria

1. WHEN requesting completion THEN the system SHALL respond within 50ms for typical workspaces
2. WHEN the workspace contains thousands of constants THEN completion performance SHALL remain consistent
3. WHEN files are modified THEN constant completion SHALL reflect updates immediately
4. WHEN multiple completion requests occur THEN the system SHALL handle them efficiently
5. WHEN memory usage grows THEN the system SHALL use efficient data structures and caching
6. WHEN the index is being updated THEN completion SHALL remain available with current data

### Requirement 5: Integration with Existing Index

**User Story:** As a maintainer of the Ruby Fast LSP, I want constant completion to seamlessly integrate with the existing indexing system so that it leverages all indexed symbol information.

#### Acceptance Criteria

1. WHEN the RubyIndex contains class entries THEN they SHALL be available for completion
2. WHEN the RubyIndex contains module entries THEN they SHALL be available for completion
3. WHEN the RubyIndex contains constant entries THEN they SHALL be available for completion
4. WHEN entries are added to the index THEN they SHALL be immediately available for completion
5. WHEN entries are removed from the index THEN they SHALL no longer appear in completion
6. WHEN mixin information is available THEN it SHALL influence constant accessibility

### Requirement 6: Completion Item Details

**User Story:** As a Ruby developer, I want detailed information about constant completion items so that I can understand what each constant represents and make informed choices.

#### Acceptance Criteria

1. WHEN a completion item is a class THEN it SHALL show class-specific information and inheritance
2. WHEN a completion item is a module THEN it SHALL show module-specific information and mixins
3. WHEN a completion item is a constant THEN it SHALL show the constant value if available
4. WHEN constants have documentation THEN it SHALL be included in completion details
5. WHEN constants are deprecated THEN this SHALL be indicated in the completion
6. WHEN constants have different visibility THEN this SHALL be reflected in the completion

### Requirement 7: Advanced Completion Features

**User Story:** As a Ruby developer, I want advanced completion features like fuzzy matching and intelligent ranking so that I can find constants quickly even with partial or imprecise input.

#### Acceptance Criteria

1. WHEN I type partial matches THEN the system SHALL support fuzzy matching for constant names
2. WHEN multiple constants match THEN they SHALL be ranked by relevance, recency, and usage
3. WHEN I use abbreviations THEN the system SHALL match against camelCase patterns (e.g., "AR" for "ActiveRecord")
4. WHEN constants are frequently used THEN they SHALL be ranked higher in completion
5. WHEN constants are in the current file THEN they SHALL be prioritized over external constants
6. WHEN I have a completion history THEN recently used constants SHALL be ranked higher

### Requirement 8: LSP Protocol Compliance

**User Story:** As a user of VS Code or other LSP-compatible editors, I want constant completion to work seamlessly with my editor's completion functionality.

#### Acceptance Criteria

1. WHEN the editor requests completion THEN the system SHALL respond with proper LSP `CompletionItem` structures
2. WHEN completion items are returned THEN they SHALL use appropriate `CompletionItemKind` values
3. WHEN additional details are available THEN they SHALL be provided via `CompletionItemLabelDetails`
4. WHEN documentation is available THEN it SHALL be included in the completion response
5. WHEN the client supports it THEN the system SHALL provide additional completion resolve information
6. WHEN completion is triggered THEN it SHALL respect LSP trigger characters and contexts

### Requirement 9: Error Handling and Edge Cases

**User Story:** As a Ruby developer, I want constant completion to handle edge cases gracefully without breaking or providing incorrect suggestions.

#### Acceptance Criteria

1. WHEN the workspace is very large THEN the system SHALL implement reasonable limits on completion items
2. WHEN completion queries are malformed THEN the system SHALL handle them gracefully without crashing
3. WHEN the index is being updated THEN completion SHALL remain available with current data
4. WHEN files have syntax errors THEN constants from parseable portions SHALL still be available
5. WHEN constants are defined dynamically THEN they SHALL be included where statically analyzable
6. WHEN circular dependencies exist THEN the system SHALL handle them without infinite loops

### Requirement 10: Testing and Validation

**User Story:** As a maintainer of the Ruby Fast LSP, I want comprehensive tests for constant completion functionality to ensure reliability and prevent regressions.

#### Acceptance Criteria

1. WHEN constant completion functionality is implemented THEN unit tests SHALL cover all constant types
2. WHEN complex namespace scenarios exist THEN integration tests SHALL verify end-to-end completion
3. WHEN edge cases are identified THEN specific test cases SHALL be added to prevent regressions
4. WHEN performance requirements exist THEN benchmark tests SHALL validate completion speed
5. WHEN the RubyIndex integration changes THEN tests SHALL verify continued compatibility
6. WHEN LSP protocol compliance is required THEN tests SHALL validate proper message formats

## Non-Functional Requirements

### Performance
- Constant completion SHALL complete within 50ms for workspaces with up to 10,000 constants
- Memory overhead for completion SHALL not exceed 5% of the RubyIndex memory usage
- Completion ranking SHALL complete within 10ms for typical result sets

### Reliability
- The system SHALL handle malformed completion requests without crashing
- Constant completion SHALL remain available during index updates
- The system SHALL maintain consistency between index state and completion results

### Usability
- Completion results SHALL be ranked by relevance with exact matches first
- Constant information SHALL provide sufficient context for disambiguation
- Completion SHALL support common developer workflows and typing patterns

### Maintainability
- Constant completion logic SHALL reuse existing RubyIndex infrastructure
- The implementation SHALL follow existing code patterns and conventions
- Completion SHALL be extensible for future enhancements

## Success Criteria

The constant completion support will be considered successful when:

1. **Performance**: Completion responds within 50ms for typical workspaces
2. **Accuracy**: 95% of constants in the index are discoverable through completion
3. **Integration**: Seamless integration with existing RubyIndex without performance degradation
4. **Usability**: Developers can find any constant in their workspace within 3 keystrokes on average
5. **Reliability**: No crashes or incorrect suggestions in production usage
6. **LSP Compliance**: Full compatibility with VS Code and other LSP clients

## Implementation Notes

### RubyIndex Integration
- Leverage `RubyIndex.definitions` map for primary constant lookup
- Filter entries by `EntryKind::Class`, `EntryKind::Module`, and `EntryKind::Constant`
- Utilize existing `FullyQualifiedName` for namespace resolution and ranking
- Reuse `Entry` structures for constant information

### LSP Protocol Implementation
- Extend existing `handle_completion` function in `src/capabilities/completion.rs`
- Use `CompletionItemKind::CLASS`, `CompletionItemKind::MODULE`, and `CompletionItemKind::CONSTANT`
- Provide detailed completion information using `CompletionItemLabelDetails`
- Support completion resolve for additional documentation and details

### Completion Algorithm Design
- Implement efficient string matching for constant names
- Support case-insensitive matching with preference for exact case matches
- Provide fuzzy matching and camelCase abbreviation support
- Implement relevance ranking based on scope proximity and usage patterns

### Context Resolution
- Use existing `RubyPrismAnalyzer` for scope and context analysis
- Leverage scope stack for namespace-aware completion
- Integrate with mixin resolution for accessible constants
- Support Ruby's constant lookup rules and precedence

## Future Extensibility
- Design for potential integration with type information and documentation
- Consider future support for constant value preview and inline documentation
- Plan for integration with refactoring tools and constant renaming
- Design for potential workspace-wide constant usage analysis