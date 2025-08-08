# Implementation Plan

- [x] 1. Implement ReceiverKind enum and update Identifier enum structure
  - Add `ReceiverKind` enum with None, SelfReceiver, Constant, and Expr variants
  - Replace existing `Identifier` enum with new structure containing namespace and context fields
  - Update `Display` implementation for the new `Identifier` enum structure
  - _Requirements: 1.1, 2.1, 2.2, 2.3, 2.4, 2.5, 4.2, 4.3_

- [x] 2. Update identifier visitor to populate new Identifier structure
  - Modify `IdentifierVisitor` to create `RubyConstant` identifiers with namespace and iden fields
  - Update method call handling to determine `ReceiverKind` and populate `RubyMethod` identifiers correctly
  - Update variable handling to use new `RubyVariable` identifier structure
  - Ensure proper context tracking for namespace stacks and scope information
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 3.1, 3.2, 3.3, 3.4, 5.1, 5.2, 5.3_

- [x] 3. Fix compilation errors in analyzer module
  - Update all pattern matching on `Identifier` enum in analyzer_prism module
  - Fix any helper functions that work with the old enum structure
  - Update utility functions that process identifier information
  - _Requirements: 4.1, 4.2, 4.3_

- [x] 4. Fix compilation errors in capabilities module
  - Update completion.rs to work with new identifier structure
  - Update definitions module (constant.rs, method.rs, variable.rs) to handle new enum variants
  - Update references.rs to work with enhanced identifier context
  - Update semantic_tokens.rs and inlay_hints.rs if they use identifier information
  - _Requirements: 4.1, 4.2, 4.3_

- [x] 5. Fix compilation errors in other modules
  - Update any other modules that import or use the `Identifier` enum
  - Fix handlers module if it processes identifier information
  - Update indexer module if it interacts with identifier types
  - _Requirements: 4.1, 4.2, 4.3_

- [x] 6. Update and fix all existing tests
  - Update unit tests in analyzer_prism/mod.rs to work with new enum structure
  - Fix integration tests that verify identifier resolution
  - Update test assertions to match new identifier format
  - Add helper functions for test assertions if needed
  - _Requirements: 4.1, 4.2, 4.3_

- [x] 7. Add comprehensive tests for ReceiverKind classification
  - Write tests for `ReceiverKind::None` (method calls without receiver)
  - Write tests for `ReceiverKind::SelfReceiver` (self.method calls)
  - Write tests for `ReceiverKind::Constant` (Class.method and Module::Class.method calls)
  - Write tests for `ReceiverKind::Expr` (variable.method and expression.method calls)
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 2.1, 2.2, 2.3, 2.4, 2.5_

- [x] 8. Add tests for enhanced constant identifier context
  - Write tests for constant resolution with namespace context in nested modules
  - Write tests for absolute constant path resolution (::TopLevel::Const)
  - Write tests for constant path precision (cursor on different parts of Foo::Bar::BAZ)
  - Write tests for constant identifiers in various namespace contexts
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 5.1, 5.4_

- [ ] 9. Add tests for method identifier context and receiver kinds
  - Write tests for method calls with different receiver types in various namespace contexts
  - Write tests for method resolution within nested classes and modules
  - Write tests for complex constant receiver scenarios (Module::Class.method)
  - Write tests verifying namespace context is correctly captured for method calls
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 5.1, 5.2, 5.4_

- [ ] 10. Add tests for variable identifier scope context
  - Write tests for local variable resolution with proper LVScopeStack context
  - Write tests for class and instance variable resolution with namespace context
  - Write tests for global variable resolution (no additional context)
  - Write tests for variable resolution in nested scopes and complex scenarios
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 5.3, 5.4_

- [ ] 11. Run comprehensive test suite and fix any remaining issues
  - Execute full test suite to ensure all tests pass
  - Fix any edge cases or issues discovered during testing
  - Verify that existing LSP functionality works correctly with new identifier structure
  - Ensure performance is acceptable with enhanced context tracking
  - _Requirements: 4.1, 4.2, 4.3, 4.4_
