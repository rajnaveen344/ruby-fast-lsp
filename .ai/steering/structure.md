# Project Structure

## Root Directory Layout

```
├── src/                    # Main LSP server source code
├── crates/                 # Additional workspace crates
│   ├── ruby-analysis/      # Core facts, graph/query engine, inference, and indexing primitives
│   ├── ast-visualizer/     # Web-based AST visualization tool
│   ├── lsp-repl/           # LSP REPL debugger
│   └── rbs-parser/         # RBS type signature parser
├── vsix/                   # VS Code extension files
│   ├── stubs/              # Ruby stdlib stubs
│   └── bin/                # Platform-specific binaries
├── scripts/                # Utility scripts
├── target/                 # Build artifacts (generated)
└── .ai/                    # AI assistant documentation
    └── steering/           # Project context for AI sessions
```

## Core Source Structure (`src/`)

### Main Components

- `main.rs` - Application entry point, CLI parsing, server startup
- `server.rs` - LSP server implementation, request routing, document cache
- `config.rs` - Configuration management

### Core Modules

- `analyzer_prism/` - Ruby code analysis using Prism parser
- `indexer/` - Symbol indexing and workspace tracking
- `capabilities/` - LSP feature implementations
- `handlers/` - Request and notification handlers
- `types/` - Core data structures and representation
- `bin/` - CLI tools and profilers
- `test/` - Integration, unit, and simulation tests
- `utils/` - Shared utilities (file ops, parser utils)
- `yard/` - YARD documentation processing

## Analyzer Module (`src/analyzer_prism/`)

```
analyzer_prism/
├── mod.rs                  # Module exports and tests
├── analyzer.rs             # RubyPrismAnalyzer implementation
├── identifier.rs           # Identifier and MethodReceiver types
├── scope_tracker.rs        # Scope and namespace tracking during traversal
├── utils.rs                # Analysis utilities
└── visitors/               # AST visitor implementations
    ├── document_symbols_visitor.rs  # Document outline extraction
    ├── empty_visitor.rs             # Base visitor pattern
    ├── identifier_visitor/          # Identifier resolution at position
    │   ├── mod.rs
    │   ├── call_node.rs
    │   ├── class_node.rs
    │   ├── constant_path_node.rs
    │   ├── def_node.rs
    │   ├── local_variable_read_node.rs
    │   └── ...
```

## Indexer Module (`src/indexer/`)

```
indexer/
├── mod.rs                  # Module exports
├── coordinator.rs          # Orchestrates indexing operations
├── file_processor.rs       # Individual file processing logic
├── fact_collector/         # Single-pass refs, diagnostics, scopes, extension patches
│   ├── mod.rs
│   ├── class_node.rs
│   ├── module_node.rs
│   ├── def_node.rs
│   ├── call_node/
│   └── ...
├── indexer_project.rs      # Project file indexing
├── indexer_stdlib.rs       # Ruby stdlib stubs indexing
├── indexer_gem.rs          # Gem dependency indexing
├── interner.rs             # String interning for memory efficiency
└── version/                # Ruby version detection
```

## Query Adapter Module (`src/query/`)

The adapter layer between handlers and `ruby-analysis::engine`. Domain queries
belong in engine; this layer keeps protocol mapping, cursor parsing, and
`TextRange -> Location` conversion.

```
query/
├── mod.rs                  # EngineQuery wrapper and module exports
├── code_lens.rs            # Code lens protocol adapter
├── completion.rs           # Completion protocol adapter
├── debug.rs                # Debug/inspection queries (lookup, stats, ancestors, etc.)
├── definition.rs           # Go-to-definition adapter
├── diagnostics.rs          # YARD and unresolved diagnostics queries
├── hover.rs                # Hover information queries
├── inference.rs            # Type inference resolvers
├── inlay_hints.rs          # Inlay hint queries
├── method.rs               # Method resolution and return types
├── namespace_tree.rs       # Namespace tree queries
├── references.rs           # Find-references queries
├── type_hierarchy.rs       # Supertype/subtype queries
├── types.rs                # Type inference utilities
└── workspace_symbols.rs    # Workspace symbol protocol adapter
```

## Capabilities Module (`src/capabilities/`)

Thin adapters that bridge handlers to the query layer. AST-only features
(no index access) live here directly; index-heavy features delegate to `query/`.

```
capabilities/
├── mod.rs                  # Module exports
├── code_lens.rs            # Code lens adapter (→ query/code_lens.rs)
├── hover.rs                # Hover adapter (→ query/hover.rs)
├── completion/             # Completion adapter (→ query/completion.rs)
├── definitions/            # Definition adapter (→ query/definition.rs)
├── diagnostics.rs          # Diagnostics adapter (→ query/diagnostics.rs)
├── document_symbols.rs     # Document outline (AST-only)
├── folding_range.rs        # Code folding (AST-only)
├── formatting.rs           # Auto-end and basic formatting (AST-only)
├── inlay_hints.rs          # Inlay hints adapter (→ query/inlay_hints.rs)
├── namespace_tree.rs       # Namespace tree adapter (→ query/namespace_tree.rs)
├── references.rs           # References adapter (→ query/references.rs)
├── semantic_tokens.rs      # Syntax highlighting (AST-only)
├── type_hierarchy.rs       # Type hierarchy adapter (→ query/type_hierarchy.rs)
├── workspace_symbols.rs    # Workspace symbols adapter (→ query/workspace_symbols.rs)
├── debug.rs                # Debug adapter (→ query/debug.rs)
└── indexing/               # Indexing-related capabilities
```

## Types Module (`src/types/`)

```
types/
├── mod.rs                    # Module exports
├── fully_qualified_name.rs   # FQN handling (Module::Class)
├── ruby_document.rs          # Document representation
├── ruby_method.rs            # Method metadata
├── ruby_namespace.rs         # Namespace types
├── ruby_version.rs           # Ruby version handling
├── scope.rs                  # Scope stack management
├── compact_location.rs       # Efficient location representation
└── unresolved_index.rs       # Handling for unresolved constants
```

## Type Inference Module (`crates/ruby-analysis/src/inference/`)

```
crates/ruby-analysis/src/inference/
├── lib.rs                  # Crate exports
├── control_flow.rs         # Structural reachability analysis
├── rbs.rs                  # RBS integration
├── method/                 # Method-specific inference
├── type_tracker/           # Local flow/type tracking and narrowing
└── type/                   # Specialized type inference
```

## Testing Structure (`src/test/`)

```
test/
├── mod.rs                  # Test module setup
├── integration_test.rs     # TestHarness and utilities
├── definitions.rs          # Go-to-definition tests
├── references.rs           # Find references tests
├── code_lens.rs            # Code lens tests
├── code_lens_transitive.rs # Transitive mixin tests
├── coordinator_test.rs     # Indexer coordinator tests
├── inlay_hints_integration.rs # Inlay hints tests
├── fixtures/               # Ruby test files
│   ├── goto/               # Go-to-definition fixtures
│   └── ...
├── snapshots/              # Insta snapshot files
└── unit/                   # Unit tests by module
```

## Naming Conventions

### Files and Modules

- Snake_case for file names: `ruby_document.rs`, `fact_collector.rs`
- Module names match file names without extension
- Visitor files named after AST node type: `class_node.rs`, `def_node.rs`

### Code Structure

- Each visitor handles one specific AST node type
- Capabilities adapt LSP requests to the query layer and engine-backed facts
- Types are shared across modules but defined centrally
- Error handling uses `anyhow::Result<T>` consistently

## Key Architectural Patterns

1. **Visitor Pattern**: Used extensively for AST traversal
2. **Fact Collection**: Single-pass AST traversal emits engine-owned facts
3. **3-Layer Architecture**: `handlers/` (API) → `query/` (Service) → `indexer/` (Data)
4. **Thin Capabilities**: `capabilities/` are adapters; business logic lives in `query/`
5. **Separation of Concerns**: Clear boundaries between parsing, indexing, analysis, and LSP features
6. **Position Translation**: Consistent byte offset → LSP position conversion
7. **FQN-based Indexing**: All symbols stored with fully qualified names
