# Tests Directory

This directory contains test files used for testing the Ruby Fast LSP server.

## Structure

- **fixtures/**: Contains Ruby files used for testing the parser, indexer, and LSP features
- **fixtures_integration_test.rs**: Integration tests for verifying that fixtures can be properly parsed and indexed
- **ruby_language_test.rs**: Tests for verifying Ruby language parsing capabilities

## Fixtures

The `fixtures/` directory contains various Ruby files that test different aspects of the Ruby language and LSP functionality. These fixtures include:

- Class and module definitions
- Method declarations and calls
- Variable declarations (local, instance, class)
- Control flow structures
- Error handling
- Blocks and procs

## Integration Tests

The integration tests focus on ensuring that:

1. The parser can successfully parse all fixture files
2. The indexer can process and index the parsed files
3. Language features like classes, methods, and variables are correctly identified

## Running Tests

Run all tests with:

```bash
cargo test
```

Run only the integration tests with:

```bash
cargo test --test fixtures_integration_test
cargo test --test ruby_language_test
```

## Adding Tests

When adding new tests:

1. Add new Ruby fixtures to the `fixtures/` directory
2. Add test functions to the appropriate test file
3. Update the `fixtures/README.md` to document the new fixtures
