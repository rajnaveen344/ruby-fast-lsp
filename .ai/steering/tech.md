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

## Build Configuration

- **Release Profile**: Optimized for size (`opt-level = "z"`) with debug symbols stripped
- **Cross-compilation**: Configured via Cross.toml for Linux and Windows targets
- **Workspace**: Multi-crate workspace with main LSP and ast-visualizer crate
