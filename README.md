# Ruby Fast LSP

A fast Language Server Protocol implementation for Ruby written in Rust.

## Features

- Syntax highlighting
- Code completion
- Hover information
- Go to definition
- Error checking

## Architecture

The LSP is built with the following components:

- **Server**: Implements the LSP protocol and handles client communication
- **Parser**: Uses tree-sitter to parse Ruby code
- **Analyzer**: Analyzes the parsed code to provide language features
- **Indexer**: Indexes Ruby symbols for fast lookup and navigation

## Development

### Prerequisites

- Rust (latest stable version)
- Cargo

### Building

```bash
cargo build
```

### Running

```bash
cargo run
```

### Testing

```bash
cargo test
```

The test suite is organized by component:

- Server tests: `src/tests/server_tests.rs`
- Parser tests: `src/tests/parser_tests.rs`
- Indexer tests: `src/tests/indexer_tests.rs`
- Analyzer tests: `src/tests/analyzer_tests.rs`
- Integration tests: `src/tests/integration_test.rs`

To run tests for a specific component:

```bash
cargo test --test server_tests
```

See `src/tests/README.md` for more details on the test suite.

## Usage

### VSCode

1. Install the Ruby Fast LSP extension
2. Open a Ruby file
3. The LSP will automatically start and provide language features

### Neovim

1. Install the Ruby Fast LSP
2. Configure Neovim to use the LSP
3. Open a Ruby file
4. The LSP will automatically start and provide language features

## License

MIT
