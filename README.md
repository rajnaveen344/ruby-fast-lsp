# Ruby Fast LSP

A fast Language Server Protocol implementation for Ruby written in Rust.

## Features

- Syntax highlighting
- Code completion
- Hover information
- Go to definition
- Error checking
- AST visualization

## Architecture

The LSP is built with the following components:

- **Server**: Implements the LSP protocol and handles client communication
- **Parser**: Uses Ruby Prism to parse Ruby code
- **Analyzer**: Analyzes the parsed code to provide language features
- **Indexer**: Indexes Ruby symbols for fast lookup and navigation
- **AST Visualizer**: Provides a visual representation of the Ruby Abstract Syntax Tree

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

### AST Visualizer

The AST Visualizer helps you understand how Ruby code is parsed into an Abstract Syntax Tree.

1. Run the visualizer script from the project root:

```bash
# Default port (8080)
./run_ast_visualizer.sh

# Or specify a custom port
./run_ast_visualizer.sh 3000
```

2. This will start the server and open the visualizer in your browser
3. Enter Ruby code in the left panel and the AST will update in real-time
4. Explore the AST in the right panel by expanding/collapsing nodes
5. You can toggle real-time parsing on/off with the checkbox

You can also start the server manually and access it in your browser:

```bash
# Start the server with default port (8080)
cargo run -p ast-visualizer

# Or specify a custom port
PORT=3000 cargo run -p ast-visualizer

# Then open in your browser (use the port shown in the server output)
open "http://localhost:8080"
```

The server will automatically find an available port if the specified port is already in use.

## License

MIT
