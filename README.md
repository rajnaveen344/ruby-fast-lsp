# Ruby Fast LSP

A high-performance Ruby Language Server written in Rust, delivering fast and accurate code navigation, syntax highlighting, and intelligent code insights for Ruby developers across entire projects.

## Features

### Feature Status vs Competitors

| **Feature**                | ruby-fast-lsp | ruby-lsp | Solargraph |
| -------------------------- | :-----------: | :------: | :--------: |
| **Syntax Highlighting**    |      ✅       |    ✅    |     ❌     |
| **Workspace Indexing**     |      ✅       |    ✅    |     ✅     |
| **Go to Definition**       |      ✅       |    ✅    |     ✅     |
| **Find References**        |    ✅ / 🚧    |    ✅    |     ✅     |
| **Code Completion**        |      ✅       |    ✅    |     ✅     |
| **Hover**                  |      ✅       |    ✅    |     ✅     |
| **Document Symbols**       |      ✅       |    ✅    |     ✅     |
| **Workspace Symbols**      |      ✅       |    ✅    |     ✅     |
| **Type Hierarchy**         |      ✅       |    ✅    |     ❌     |
| **Inlay Hints**            |      ✅       |    ✅    |     ❌     |
| **Code Folding**           |      ✅       |    ✅    |     ✅     |
| **Diagnostics**            |      ✅       |    ✅    |     ✅     |
| **Semantic Tokens**        |      ✅       |    ✅    |     ❌     |
| **Code Lens**              |      ✅       |    ✅    |     ❌     |
| **On-Type Formatting**     |      ✅       |    ✅    |     ❌     |
| **Rename (local vars)**    |      ✅       |    ✅    |     ✅     |
| **Signature Help**         |      ❌       |    ✅    |     ✅     |
| **Code Actions**           |      ❌       |    ✅    |     ❌     |
| **Document Highlight**     |      ❌       |    ✅    |     ✅     |
| **Selection Range**        |      ❌       |    ✅    |     ❌     |
| **Call Hierarchy**         |      ❌       |   🚧    |     ❌     |
| **Cross-file Rename**      |      ❌       |    ✅    |     ✅     |
| **Formatting (RuboCop)**   |      ❌       |    ✅    |     ✅     |
| **Rails Support**          |      ❌       |    ✅    |     🚧     |
| **Metaprogramming / DSLs** |      ❌       |    ✅    |     🚧     |
| **YARD Docs in Hover**     |      ❌       |    ✅    |     ✅     |
| **ERB / HAML Support**     |      ❌       |    ✅    |     ❌     |

### Navigation Details

- **Go to Definition**: Modules ✅ · Classes ✅ · Constants ✅ · Local variables ✅ · Methods ✅ · Instance/Class/Global variables 🚧
- **Find References**: Modules ✅ · Classes ✅ · Constants ✅ · Local variables ✅ · Methods 🚧 (limited coverage)

### What's Working Well

- **16 LSP features implemented** — strong core feature set
- **Performance** — Rust-native with sub-millisecond completions via trie lookups
- **Type inference engine** — RBS-backed with generic substitution (e.g., `Array[Integer]#first` → `Integer`)
- **Semantic tokens** and **inlay hints** — features competitors often lack

### Known Limitations

- **Method references** — incomplete; may miss matches across files
- **Type inference** — expanding but not yet comprehensive (no flow-sensitive typing, limited YARD)
- **Rename** — local variables only, no cross-file rename for methods/constants
- **No metaprogramming awareness** — `attr_accessor`, `define_method`, `method_missing`, Rails DSLs not recognized
- **No formatter integration** — no RuboCop/Standard delegation
- **No signature help** — no parameter hints on method calls

### Roadmap

**High Priority** (biggest impact for daily use):
- Signature help (parameter hints)
- Document highlight (same-symbol occurrences)
- Cross-file rename (methods, constants, classes)
- YARD documentation in hover

**Medium Priority** (competitive parity):
- Code actions / Quick fixes
- Formatting integration (RuboCop/Standard)
- Metaprogramming support (`attr_accessor`, `define_method`)
- Selection range (expand/shrink selection)

**Future** (differentiation):
- Rails support (routes, associations, callbacks, ERB)
- Call hierarchy (incoming/outgoing calls)
- Flow-sensitive type narrowing (`is_a?`, `nil?` guards)
- Run/Debug support

## Getting Started

1. Install the extension from the VS Code marketplace
2. Open a Ruby project folder in VS Code
3. The LSP will automatically:
   - Start up and index your workspace
   - Provide language features as you type
   - Support navigation across your entire project

### Requirements

- VS Code 1.86.0 or later
- Ruby project (single file or workspace)

## Configuration

The extension supports the following settings:

- `ruby-fast-lsp.codeLensModulesEnabled` - Enable/disable code lens for module mixin counts (default: true)

## Performance

Ruby Fast LSP is designed for speed:

- Written in Rust for native performance
- Incremental indexing on file changes
- Efficient symbol lookups using trie data structures
- Optimized for large codebases

## Known Issues

- Method references may be incomplete across files
- Metaprogramming constructs (`attr_accessor`, `define_method`, etc.) are not indexed
- Large workspaces may take time for initial indexing
- Some Ruby edge cases (complex splatting, pattern matching) may not be fully supported

## Development

### Building from Source

```bash
cargo build --release
```

### Running Tests

```bash
# Run all tests
cargo test

# Run simulation tests (property-based fuzzing)
cargo test sim --release

# Run soak test (overnight fuzzing, Ctrl+C to stop)
cargo test soak --release -- --nocapture --ignored
```

See [docs/testing.md](docs/testing.md) for detailed testing documentation.

### Project Structure

- `src/` - Main LSP server implementation
- `src/capabilities/` - LSP feature handlers (completion, hover, definitions, etc.)
- `src/indexer/` - Project indexing and symbol management
- `src/inferrer/` - Type inference engine
- `src/analyzer_prism/` - Ruby code analysis using Prism
- `src/types/` - Core data structures and representation
- `src/handlers/` - Request and notification routing
- `src/test/simulation/` - Property-based simulation tests
- `crates/ast-visualizer/` - Web-based AST visualization tool
- `crates/rbs-parser/` - RBS type signature parser
- `crates/lsp-repl/` - LSP REPL for debugging
- `vsix/` - VS Code extension packaging

## Contributing

Please report any issues or feature requests on our [GitHub repository](https://github.com/rajnaveen344/ruby-fast-lsp).

## License

MIT
