# Technology Stack

## Core Technologies

- **Language**: Rust (2021 edition)
- **Ruby Parser**: ruby-prism 1.4.0 for AST parsing and analysis
- **LSP Framework**: tower-lsp 0.19.0 for Language Server Protocol implementation
- **Async Runtime**: tokio 1.32.0 with full features for async operations
- **Serialization**: serde 1.0.188 with derive features for JSON handling

## Key Dependencies

- **Concurrency**: parking_lot 0.12.4 for efficient mutexes and locks
- **File Operations**: walkdir 2.4.0 for workspace traversal
- **Logging**: log 0.4.20 + env_logger 0.10.0 for structured logging
- **Error Handling**: anyhow 1.0.75 for error management
- **Web Server**: actix-web 4.3.1 + actix-cors 0.6.4 for AST visualizer

## Development Tools

- **Testing**: insta 1.43.1 for snapshot testing, pretty_assertions 1.4.0
- **Cross-compilation**: cross-rs for multi-platform builds
- **Build**: cc 1.0.83 for native compilation

## Common Commands

### Development
```bash
# Run the LSP server
cargo run

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Update snapshots
cargo insta review
```

### Building
```bash
# Debug build
cargo build

# Release build
cargo build --release

# Cross-compile for specific target
cross build --release --target x86_64-unknown-linux-gnu
```

### AST Visualizer
```bash
# Start AST visualizer (default port 8080)
./run_ast_visualizer.sh

# Start on custom port
./run_ast_visualizer.sh 3000
```

### VS Code Extension
```bash
# Create VSIX package for current platform
./create_vsix.sh --current-platform-only

# Create VSIX for all platforms
./create_vsix.sh --platforms all

# Rebuild and package
./create_vsix.sh --rebuild
```

## Critical Implementation Notes

### LSP Position Handling
- **⚠️ CRITICAL**: LSP uses **0-indexed** line and character positions
- **Common Bug**: Test definitions often use 1-indexed positions by mistake
- **Position Structure**: `Position { line: 0, character: 0 }` = first line, first character
- **Range Structure**: `Range { start: Position, end: Position }` where end is exclusive
- **Debugging**: Always verify line numbers match between test expectations and actual AST positions

### LSP Tools MCP Integration
- **MCP Server**: Integrated LSP Tools MCP for enhanced development capabilities
- **Available Tools**: 
  - `mcp_LSP_Tools_find_regex_position`: Find regex matches with precise positions
  - `mcp_LSP_Tools_list_allowed_directories`: List accessible directories
- **Position Format**: All MCP tools return 0-indexed positions consistent with LSP protocol
- **Usage**: Useful for debugging position-related issues and validating AST node locations

### Testing Best Practices
- **Snapshot Testing**: Use `cargo insta review` to accept/reject test changes
- **Position Verification**: Always use `cat -n` to verify actual line numbers in test files
- **Debug Output**: Add temporary debug prints to trace position calculations
- **Line Counting**: Remember that blank lines and comment lines count in line numbering

## Build Configuration

- **Release Profile**: Optimized for size (`opt-level = "z"`) with debug symbols stripped
- **Cross-compilation**: Configured via Cross.toml for Linux and Windows targets
- **Workspace**: Multi-crate workspace with main LSP and ast-visualizer crate
