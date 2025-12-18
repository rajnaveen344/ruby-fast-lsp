# Ruby Fast LSP

Ruby Fast LSP is a high-performance Language Server Protocol (LSP) implementation for Ruby, written in Rust. It provides fast and accurate code navigation, syntax highlighting, and intelligent code insights for Ruby developers across entire projects.

## Core Features

### Implemented Features

- **Workspace Indexing**

  - Two-phase indexing (definitions → references) to ensure accurate cross-file resolution.
  - Automatic indexing of Ruby project files, stdlib stubs (1.8 - 3.4), and gem dependencies.
  - Parallelized processing for high performance on large codebases.
  - Incremental re-indexing on file changes with document version tracking.

- **Code Navigation**

  - Fast Go-to-definition for classes, modules, constants, local variables, and methods.
  - Find-all-references for constants, classes, modules, local variables, and methods.
  - Cross-file navigation with support for transitive mixins and inheritance.

- **Syntax Highlighting**

  - Semantic token-based highlighting with full coverage of Ruby constructs.

- **Code Completion**

  - Local variable, constant, class, and module completions with fuzzy matching.
  - Scope resolution (`::`) and context-aware snippets.
  - Prefix tree (Trie) based lookups for sub-millisecond completion latency.

- **Document symbols & Workspace symbols**

  - Nested symbol hierarchy with visibility and kind info.
  - Fuzzy search across the entire workspace with camelCase and subsequence matching.

- **Inlay Hints & Code Folding**

  - End keyword hints and local variable type hints.
  - Comprehensive code folding for all block-level constructs.

- **Diagnostics**

  - Syntax errors from the Prism parser and unresolved constant diagnostics post-indexing.

- **Code Lens**

  - Module mixin usage counts and navigation (include/prepend/extend).

- **On-Type Formatting**
  - Instant `end` keyword insertion.

### Planned Features

- Hover information
- Code actions / Quick fixes
- Rename support
- Formatting integration (Rubocop)
- Instance/class/global variable enhancements
- Expanded Type inference (RBS/YARD integration)
- Meta-programming support
- Run/Debug support

## Architecture Philosophy

The project follows a modular architecture optimized for speed and low memory overhead:

- **Indexer**: High-performance symbol storage.

  - **Memory Efficiency**: Interns FQNs and URIs using `SlotMap` and `Ustr` to minimize allocations.
  - **Two-Phase Protocol**: Indexes definitions first, then references, avoiding race conditions during startup.
  - **Parallelization**: Utilizes a custom parallelizer for multi-threaded indexing.

- **Analyzer**: Semantic engine powered by the Prism parser.

  - Visitor-based AST traversal for modular feature implementation.
  - Robust scope tracking for complex Ruby namespace resolution.

- **Capabilities**: LSP features built on top of the Indexer and Analyzer.

  - Decoupled implementations that share common position translation logic.

- **Server**: Async protocol handler.
  - Efficient document caching and incremental update handling.

## Target Users

Ruby developers using VS Code or other LSP-compatible editors who need fast, accurate code navigation and analysis across large Ruby codebases.
