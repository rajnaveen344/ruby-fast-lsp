# Tests Directory

This directory contains test files used for testing the Ruby Fast LSP server.

## Structure

- **fixtures/**: Contains Ruby files used for testing the parser, indexer, and LSP features
- **integration_test.rs**: Integration tests for verifying fixture availability and basic integration points

## Fixtures

The `fixtures/` directory contains various Ruby files that test different aspects of the Ruby language and LSP functionality. These fixtures include:

- Class and module definitions
- Method declarations and calls
- Variable declarations (local, instance, class)
- Control flow structures
- Error handling
- Blocks and procs

Additionally, there are LSP-specific fixtures for testing:
- Definition/goto functionality
- References
- Symbols
- Completion
- Hover information

## Integration Tests

The integration tests focus on ensuring that all LSP related functionalities(goto, references, completion, etc.) is working as expected for all fixtures.

## Running Tests

Run all tests with:

```bash
cargo test
```

Run only the integration tests with:

```bash
cargo test --test integration_test
```

## Adding Tests

When adding new tests:

1. Add new Ruby fixtures to the `fixtures/` directory
2. Add test functions to the appropriate test file

### Adding LSP Integration Tests

For future LSP-specific integration tests:

1. Focus on testing the integration between our Ruby indexing/parsing and the LSP protocol
2. Avoid duplicating tests already covered by the `tower_lsp` crate
3. Use the provided fixtures to test specific LSP features like definition, references, etc.
