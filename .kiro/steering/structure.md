# Project Structure

## Root Directory Layout

```
├── src/                    # Main LSP server source code
├── crates/                 # Additional workspace crates
│   └── ast-visualizer/     # Web-based AST visualization tool
├── vsix/                   # VS Code extension files
├── docs/                   # Technical documentation
├── target/                 # Build artifacts (generated)
└── test_method.rb          # Test file for development
```

## Core Source Structure (`src/`)

### Main Components
- `main.rs` - Application entry point and server initialization
- `server.rs` - LSP server implementation and request routing

### Core Modules
- `analyzer_prism/` - Ruby code analysis using Prism parser
- `indexer/` - Symbol indexing and workspace tracking
- `capabilities/` - LSP feature implementations
- `handlers/` - Request and notification handlers
- `types/` - Core data structures and types
- `test/` - Integration and unit tests

## Analyzer Module (`src/analyzer_prism/`)

```
analyzer_prism/
├── mod.rs                  # Main analyzer interface and Identifier enum
├── scope_tracker.rs        # Scope and namespace tracking
├── utils.rs               # Analysis utilities
└── visitors/              # AST visitor implementations
    ├── empty_visitor.rs    # Base visitor pattern
    ├── identifier_visitor.rs # Identifier resolution
    ├── index_visitor/      # Symbol indexing visitors
    ├── inlay_visitor.rs    # Inlay hints generation
    ├── reference_visitor.rs # Reference finding
    └── token_visitor.rs    # Semantic token generation
```

## Indexer Module (`src/indexer/`)

```
indexer/
├── mod.rs                 # Module exports
├── index.rs              # Core index data structure
├── ancestor_chain.rs     # Namespace hierarchy tracking
└── entry/               # Index entry definitions
    ├── mod.rs
    ├── entry_builder.rs  # Builder pattern for entries
    └── entry_kind.rs     # Entry type definitions
```

## Capabilities Module (`src/capabilities/`)

```
capabilities/
├── mod.rs                # Module exports
├── completion.rs         # Code completion
├── inlay_hints.rs       # Inlay hints
├── references.rs        # Find references
├── semantic_tokens.rs   # Syntax highlighting
└── definitions/         # Go-to-definition implementations
    ├── mod.rs
    ├── constant.rs      # Constant definitions
    ├── method.rs        # Method definitions
    └── variable.rs      # Variable definitions
```

## Types Module (`src/types/`)

```
types/
├── mod.rs                    # Module exports
├── fully_qualified_name.rs  # FQN handling
├── ruby_document.rs         # Document representation
├── ruby_method.rs           # Method metadata
├── ruby_namespace.rs        # Namespace/constant types
├── ruby_variable.rs         # Variable types
└── scope.rs                 # Scope stack management
```

## Testing Structure (`src/test/`)

```
test/
├── mod.rs                # Test module setup
├── integration_test.rs   # End-to-end LSP tests
├── definitions.rs        # Definition tests
├── references.rs         # Reference tests
├── fixtures/            # Ruby test files
├── snapshots/           # Insta snapshot files
└── unit/               # Unit tests by module
    └── definitions/     # Definition-specific unit tests
```

## Naming Conventions

### Files and Modules
- Snake_case for file names: `ruby_document.rs`, `index_visitor.rs`
- Module names match file names without extension
- Test files end with `_test.rs` for unit tests

### Code Structure
- Each visitor handles one specific AST traversal concern
- Capabilities combine indexer and analyzer functionality
- Types are shared across modules but defined centrally
- Error handling uses `anyhow::Result<T>` consistently

## Key Architectural Patterns

1. **Visitor Pattern**: Used extensively for AST traversal
2. **Builder Pattern**: Used for constructing index entries
3. **Service Layer**: Indexer and analyzer act as services to capabilities
4. **Separation of Concerns**: Clear boundaries between parsing, indexing, analysis, and LSP features
