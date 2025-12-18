# Technology Stack

## Core Technologies

- **Language**: Rust (2021 edition)
- **Ruby Parser**: ruby-prism 1.4.0 for AST parsing and analysis
- **LSP Framework**: tower-lsp 0.20.0 for Language Server Protocol implementation
- **Async Runtime**: tokio 1.32.0 with full features for async operations
- **Serialization**: serde 1.0.188 + serde_json 1.0.107 for JSON handling

## Key Dependencies

### Runtime

- **Concurrency**: parking_lot 0.12.4 for efficient mutexes and locks
- **Parallelization**: rayon-style custom parallelizer for indexing
- **File Operations**: walkdir 2.4.0 for workspace traversal
- **Logging**: log 0.4.20 + env_logger 0.10.0 for structured logging
- **Error Handling**: anyhow 1.0.75 for error management
- **CLI**: clap 4.4.0 with derive features for command-line parsing
- **Data Structures**:
  - slotmap 1.0 for high-performance ID-based storage (interning)
  - ustr 1.0 for fast, static-lifetime string interning
  - trie-rs 0.4.2 for prefix tree lookups
- **Date/Time**: chrono 0.4.31 with serde support
- **Monitoring**: dhat 0.3.2 for heap profiling and memory analysis
- **Unicode**: unicode-ident 1.0.18 for identifier validation

### Platform-Specific

- **Unix**: libc 0.2 for process monitoring
- **Windows**: windows-sys 0.59 for process monitoring

### AST Visualizer (separate crate)

- **Web Server**: actix-web 4.3.1 + actix-cors 0.6.4

## Development Tools

- **Testing**:

  - insta 1.43.1 for snapshot testing (with json, redactions features)
  - pretty_assertions 1.4.0 for readable test diffs
  - tokio-test 0.4.3 for async testing
  - tempfile 3.8.0 for temporary test files
  - async-trait 0.1.73 for async test traits

- **Cross-compilation**: cross-rs for multi-platform builds
- **Build**: cc 1.0.83 for native compilation

## Common Commands

### Development

```bash
# Run the LSP server directly
cargo run

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Update snapshots
cargo insta review

# Check for issues without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Cross-compile for specific target
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target x86_64-pc-windows-gnu
cross build --release --target x86_64-apple-darwin
cross build --release --target aarch64-apple-darwin
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
- **Prism Offsets**: ruby-prism uses byte offsets, must convert to LSP positions

### Byte Offset to Position Conversion

```rust
// Example pattern for conversion
fn offset_to_position(content: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for ch in content.chars() {
        if current_offset >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
        current_offset += ch.len_utf8();
    }

    Position { line, character }
}
```

### FQN (Fully Qualified Name) Convention

- Top-level constants are stored without any artificial prefix (matches Ruby's internal representation)
- Example: `String` → `String`, `MyModule::MyClass` → `MyModule::MyClass`
- FQN parsing handles both absolute (`::Foo`) and relative (`Foo`) references

### Testing Best Practices

- **Snapshot Testing**: Use `cargo insta review` to accept/reject test changes
- **Position Verification**: Double-check line numbers in test files (0-indexed!)
- **Fixture Files**: Keep fixtures in `src/test/fixtures/`
- **Snapshot Files**: Stored in `src/test/snapshots/`

## Build Configuration

### Release Profile

```toml
[profile.release]
opt-level = "z"    # Optimize for size
strip = true       # Strip debug symbols
```

### Cross-compilation

- Configured via `Cross.toml` for Linux and Windows targets
- macOS builds require native macOS environment

### Workspace Structure

- Multi-crate workspace with main LSP and ast-visualizer crate
- Shared resolver version 2

## VS Code Extension Structure

```
vsix/
├── extension.js      # Extension entry point
├── package.json      # Extension manifest
├── bin/              # Platform-specific LSP binaries
│   ├── linux-x64/
│   ├── macos-arm64/
│   ├── macos-x64/
│   └── win32-x64/
└── stubs/            # Ruby stdlib stubs
    ├── rubystubs18/
    ├── rubystubs19/
    ...
    └── rubystubs34/
```
