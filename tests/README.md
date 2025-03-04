# Ruby Fast LSP Tests

This directory contains tests for the Ruby Fast LSP server. The tests are organized into several files:

## Test Files

- **parser_tests.rs**: Tests for the Ruby parser and document handling
- **analyzer_tests.rs**: Tests for the Ruby code analyzer
- **server_tests.rs**: Tests for the LSP server implementation
- **integration_tests.rs**: Integration tests that verify the interaction between components
- **test_helpers.rs**: Helper functions and sample Ruby code for tests

## Running Tests

To run all tests:

```bash
cargo test
```

To run a specific test file:

```bash
cargo test --test parser_tests
```

To run a specific test:

```bash
cargo test --test parser_tests test_parser_initialization
```

## Test Coverage

The tests cover the following aspects of the LSP:

1. **Parser**:
   - Initialization
   - Parsing valid Ruby code
   - Handling invalid Ruby code
   - Document operations

2. **Analyzer**:
   - Initialization
   - Hover information
   - Code completions
   - Node-to-range conversion
   - Definition finding

3. **Server**:
   - Initialization
   - Document synchronization
   - Hover requests
   - Completion requests
   - Handler initialization

4. **Integration**:
   - Parser and analyzer integration
   - Handlers with documents
   - Full LSP workflow

## Adding New Tests

When adding new tests, follow these guidelines:

1. Place tests in the appropriate file based on the component being tested
2. Use the helper functions in `test_helpers.rs` for common operations
3. Follow the existing naming convention: `test_component_functionality`
4. Use `assert!` and `assert_eq!` for verifications
5. Add integration tests for new features that span multiple components
