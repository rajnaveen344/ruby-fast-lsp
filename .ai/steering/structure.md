# Project Structure

## Root Directory Layout

```
├── src/                    # Main LSP server source code
├── crates/                 # Additional workspace crates
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
- `inferrer/` - Type inference engine and RBS integration
- `handlers/` - Request and notification handlers
- `types/` - Core data structures and representation
- `bin/` - CLI tools and profilers
- `test/` - Integration, unit, and simulation tests
- `utils/` - Shared utilities (file ops, parser utils)
- `yard/` - YARD documentation processing

## Analyzer Module (`src/analyzer_prism/`)

```
analyzer_prism/
├── mod.rs                  # Main analyzer interface, Identifier enum
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
    ├── index_visitor/               # Symbol indexing visitors
    │   ├── mod.rs
    │   ├── class_node.rs
    │   ├── module_node.rs
    │   ├── def_node.rs
    │   ├── constant_write_node.rs
    │   ├── local_variable_write_node.rs
    │   ├── instance_variable_write_node.rs
    │   ├── class_variable_write_node.rs
    │   ├── global_variable_write_node.rs
    │   └── ...
    ├── reference_visitor/           # Reference finding visitors
    │   ├── mod.rs
    │   ├── constant_read_node.rs
    │   ├── constant_path_node.rs
    │   ├── local_variable_read_node.rs
    │   └── ...
    ├── inlay_visitor.rs             # Inlay hints generation
    └── token_visitor.rs             # Semantic token generation
```

## Indexer Module (`src/indexer/`)

```
indexer/
├── mod.rs                  # Module exports
├── index.rs                # Core RubyIndex data structure
├── coordinator.rs          # Orchestrates indexing operations
├── graph.rs                # Symbol dependency graph
├── indexer_project.rs      # Project file indexing
├── indexer_stdlib.rs       # Ruby stdlib stubs indexing
├── indexer_gem.rs          # Gem dependency indexing
├── index_ref.rs            # Reference indexing
├── file_processor.rs       # Individual file processing logic
├── interner.rs             # String interning for memory efficiency
├── prefix_tree.rs          # Trie for completion lookups
├── version/                # Ruby version detection
└── entry/                  # Index entry definitions
    ├── mod.rs
    ├── entry_builder.rs    # Builder pattern for entries
    └── entry_kind.rs       # Entry type definitions
```

## Capabilities Module (`src/capabilities/`)

```
capabilities/
├── mod.rs                  # Module exports
├── code_lens.rs            # Code lens (mixin usage counts)
├── hover.rs                # Documentation and type hover
├── completion/             # Code completion engine
├── definitions/            # Go-to-definition (constant, method, variable)
├── diagnostics.rs          # Syntax and semantic diagnostics
├── document_symbols.rs     # Document outline
├── folding_range.rs        # Code folding
├── formatting.rs           # Auto-end and basic formatting
├── inlay_hints.rs          # Inlay hints
├── namespace_tree.rs       # Namespace tree navigation
├── references.rs           # Find references
├── semantic_tokens.rs      # Syntax highlighting
├── type_hierarchy.rs       # Superclass/Subclass navigation
├── workspace_symbols.rs    # Workspace-wide symbol search
├── debug.rs                # Debugging utilities
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

## Type Inference Module (`src/type_inference/`)

```
inferrer/
├── mod.rs                  # Module exports
├── ruby_type.rs            # Type representation
├── query.rs                # Type query engine
├── rbs.rs                  # RBS integration
├── return_type.rs          # Method return type inference
├── cfg/                    # Control Flow Graph for inference
├── method/                 # Method-specific inference
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

- Snake_case for file names: `ruby_document.rs`, `index_visitor.rs`
- Module names match file names without extension
- Visitor files named after AST node type: `class_node.rs`, `def_node.rs`

### Code Structure

- Each visitor handles one specific AST node type
- Capabilities combine indexer and analyzer functionality
- Types are shared across modules but defined centrally
- Error handling uses `anyhow::Result<T>` consistently

## Key Architectural Patterns

1. **Visitor Pattern**: Used extensively for AST traversal
2. **Builder Pattern**: Used for constructing index entries
3. **Service Layer**: Indexer and analyzer act as services to capabilities
4. **Separation of Concerns**: Clear boundaries between parsing, indexing, analysis, and LSP features
5. **Position Translation**: Consistent byte offset → LSP position conversion
6. **FQN-based Indexing**: All symbols stored with fully qualified names
