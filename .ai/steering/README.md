# Ruby Fast LSP - AI Steering Documentation

This folder contains context and guidance for AI assistants working on the Ruby Fast LSP project. Use this as your starting point for understanding the project.

## Quick Start

**What is this project?** A high-performance Ruby Language Server Protocol (LSP) implementation written in Rust, providing code intelligence features for Ruby developers.

**Tech Stack:** Rust + ruby-prism parser + tower-lsp framework

**Key Entry Points:**

- `src/main.rs` - Application entry point
- `src/server.rs` - LSP server core
- `src/handlers/` - Request/Notification routing
- `src/capabilities/` - Feature implementations (hover, completion, etc.)
- `src/indexer/` - Symbol indexing system
- `src/inferrer/` - Type inference engine
- `src/analyzer_prism/` - AST analysis using Prism

## Documentation Index

| File                                         | Purpose                                                                      | When to Read                                               |
| -------------------------------------------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------- |
| [product.md](./product.md)                   | Feature overview, architecture philosophy, and current implementation status | Understanding what the LSP does and its design principles  |
| [structure.md](./structure.md)               | Project directory layout and code organization                               | Navigating the codebase, understanding where code lives    |
| [tech.md](./tech.md)                         | Technology stack, dependencies, and common commands                          | Building, testing, and understanding technical constraints |
| [testing.md](./testing.md)                   | Testing strategy, test harness, and fixtures                                 | Writing tests, understanding test patterns                 |
| [ruby-ast-mapping.md](./ruby-ast-mapping.md) | Ruby language features to Prism AST node types                               | Working with Ruby parsing and analysis                     |
| [tigerstyle.md](./tigerstyle.md)             | TigerStyle coding principles (safety, performance, DX)                       | Writing code, reviewing PRs, understanding code standards  |

## Current Feature Status (v0.1.0)

### ✅ Fully Implemented

- **Workspace Indexing**: Two-phase indexing (definitions then references) for project files, stdlib stubs, and gem dependencies. Parallelized for performance.
- **Go-to-definition**: Classes, modules, constants, local variables, methods.
- **Find references**: Classes, modules, constants, local variables, methods.
- **Semantic tokens**: Full syntax highlighting support.
- **Code completion**: Local variables, constants, classes, modules, snippets (with scope resolution).
- **Document symbols**: Nested hierarchy with visibility info.
- **Workspace symbols**: Fuzzy search across all indexed symbols using a prefix tree.
- **Type Hierarchy**: Support for superclass and subclass navigation.
- **Hover**: Quick info and documentation for symbols.
- **Inlay hints**: End keyword hints for blocks and type hints for local variables.
- **Code folding**: Classes, modules, methods, control flow, arrays, hashes.
- **Diagnostics**: Syntax errors from prism and unresolved constant/method diagnostics.
- **Code lens**: Module mixin usage (include/prepend/extend counts).
- **Formatting**: On-type `end` insertion and basic source formatting.
- **Simulation Testing**: Property-based testing for LSP consistency.

### 🚧 In Progress / Limited

- Method references (performance optimizations ongoing)
- Advanced Type inference (Infrastructure in place, expanding coverage)

### ❌ Not Yet Implemented

- Code actions / Quick fixes
- Rename support
- Full Rubocop integration
- Meta-programming support
- Run/Debug support

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    LSP Protocol Layer                       │
│                (tower-lsp + server.rs)                      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Request/Notification                     │
│                (request.rs,notification.rs)                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌───────────────────┐  ┌───────────────┐  ┌───────────────────┐
│   Capabilities    │  │   Inferrer    │  │      Indexer      │
│ (src/capabilities)│─▶│(src/inferrer) │◀─│   (src/indexer)   │
└───────────────────┘  └───────────────┘  └───────────────────┘
          │                   │
          ▼                   ▼
┌─────────────────────────────────────────────────────────────┐
│                    Analyzer (Prism)                         │
│                 (src/analyzer_prism/)                       │
└─────────────────────────────────────────────────────────────┘
```

## Common Development Tasks

### Running Tests

```bash
cargo test                    # Run all tests
cargo test -- --nocapture     # With output
cargo insta review            # Update snapshots
```

### Building

```bash
cargo build                   # Debug build
cargo build --release         # Release build
```

### VS Code Extension

```bash
./create_vsix.sh --current-platform-only   # Build VSIX
```

## Key Concepts

1. **FullyQualifiedName (FQN)**: All symbols are tracked with fully qualified names (e.g., `MyModule::MyClass`)

2. **Visitor Pattern**: AST traversal uses visitors for different concerns (indexing, references, tokens)

3. **Scope Stack**: Tracks current namespace context during AST traversal

4. **Index Structure**: HashMap-based with FQN keys, supporting prefix lookups for completion

## Important Notes for AI Assistants

- **LSP positions are 0-indexed** - Common source of bugs in tests
- **Prism AST nodes use byte offsets** - Must convert to LSP positions
- **Snapshot tests** - Many tests use `insta` for snapshot testing
- **Cross-platform** - Code must work on macOS, Linux, and Windows
