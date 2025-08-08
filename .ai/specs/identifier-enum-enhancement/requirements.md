# Requirements Document

## Introduction

This feature enhances the Ruby Fast LSP's identifier system by restructuring the `Identifier` enum to capture comprehensive contextual information at any cursor position. The current system provides basic identifier information but lacks the detailed context needed for precise code navigation and analysis. This enhancement introduces a new structure that captures namespace stacks, receiver information, constant paths, and scope information for each identifier type, enabling more accurate go-to-definition, hover, and reference finding capabilities.

## Requirements

### Requirement 1

**User Story:** As a Ruby developer using the LSP, I want the system to capture complete contextual information for constant identifiers so that navigation works precisely for nested constant paths.

#### Acceptance Criteria

1. WHEN analyzing `Foo::Bar::BAZ` at position of `BAZ` THEN the system SHALL create `RubyConstant` with namespace stack and identifier path `[Foo, Bar, BAZ]`
2. WHEN analyzing `Foo::Bar::BAZ` at position of `Bar` THEN the system SHALL create `RubyConstant` with namespace stack and identifier path `[Foo, Bar]`
3. WHEN analyzing `Foo::Bar::BAZ` at position of `Foo` THEN the system SHALL create `RubyConstant` with namespace stack and identifier path `[Foo]`
4. WHEN analyzing a constant inside a module/class THEN the system SHALL capture the current namespace stack context
5. WHEN analyzing an absolute constant path `::TopLevel::Const` THEN the system SHALL handle root namespace correctly

### Requirement 2

**User Story:** As a Ruby developer, I want the system to provide comprehensive method context including namespace, receiver information, and method details so that method navigation works accurately across different receiver types.

#### Acceptance Criteria

1. WHEN analyzing `method_a` (no receiver) THEN the system SHALL create `RubyMethod` with namespace stack, `ReceiverKind::None`, and method identifier
2. WHEN analyzing `self.method_a` THEN the system SHALL create `RubyMethod` with namespace stack, `ReceiverKind::SelfReceiver`, and method identifier
3. WHEN analyzing `Class.method_a` THEN the system SHALL create `RubyMethod` with namespace stack, `ReceiverKind::Constant`, and method identifier
4. WHEN analyzing `Module::Class.method_a` THEN the system SHALL create `RubyMethod` with namespace stack, `ReceiverKind::Constant`, and method identifier
5. WHEN analyzing `a.method_a` or `(a + b).method_a` THEN the system SHALL create `RubyMethod` with namespace stack, `ReceiverKind::Expr`, and method identifier
6. WHEN the method is called within a class/module THEN the system SHALL capture the current namespace stack

### Requirement 3

**User Story:** As a Ruby developer, I want the system to capture appropriate scope information for variable identifiers so that variable resolution works correctly across different variable types.

#### Acceptance Criteria

1. WHEN analyzing a local variable THEN the system SHALL create `RubyVariable` with the local variable scope stack
2. WHEN analyzing a class variable THEN the system SHALL create `RubyVariable` with the current namespace stack
3. WHEN analyzing an instance variable THEN the system SHALL create `RubyVariable` with the current namespace stack
4. WHEN analyzing a global variable THEN the system SHALL create `RubyVariable` with no additional context (global scope)
5. WHEN variables are accessed within nested scopes THEN the system SHALL capture the appropriate scope context

### Requirement 4

**User Story:** As a maintainer of the Ruby Fast LSP, I want the enhanced identifier system to maintain backward compatibility while providing comprehensive contextual information so that existing functionality continues to work.

#### Acceptance Criteria

1. WHEN the new `Identifier` enum structure is implemented THEN all existing tests SHALL continue to pass
2. WHEN accessing identifier information THEN the system SHALL provide both the identifier details and contextual information
3. WHEN the system processes any identifier type THEN it SHALL correctly populate all relevant contextual fields
4. WHEN existing code accesses identifier variants THEN it SHALL work without breaking changes

### Requirement 5

**User Story:** As a Ruby developer, I want the system to handle complex nested scenarios correctly so that navigation works in sophisticated Ruby code structures.

#### Acceptance Criteria

1. WHEN analyzing identifiers within nested modules and classes THEN the system SHALL maintain accurate namespace stacks
2. WHEN analyzing method calls on complex constant receivers THEN the system SHALL extract full constant paths correctly
3. WHEN analyzing variables in nested method and block scopes THEN the system SHALL maintain accurate scope stacks
4. WHEN the cursor is positioned on different parts of complex expressions THEN the system SHALL provide precise contextual information
