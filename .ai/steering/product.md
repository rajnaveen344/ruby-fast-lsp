# Ruby Fast LSP

Ruby Fast LSP is a high-performance Language Server Protocol (LSP) implementation for Ruby, written in Rust. It provides fast and accurate code navigation, syntax highlighting, and intelligent code insights for Ruby developers across entire projects.

## Core Features

### Implemented Features

- **Workspace Indexing**

  - Automatic indexing of Ruby project files
  - Ruby stdlib stubs (Ruby 1.8 - 3.4)
  - Gem dependency indexing
  - Incremental re-indexing on file changes

- **Code Navigation**

  - Go-to-definition for classes, modules, constants, local variables
  - Go-to-definition for methods (basic support)
  - Find-all-references for classes, modules, constants, local variables
  - Cross-file navigation support

- **Syntax Highlighting**

  - Semantic token-based highlighting
  - Full coverage of Ruby language constructs

- **Code Completion**

  - Local variable completions
  - Constant/class/module completions with fuzzy matching
  - Scope resolution (`::`operator) completions
  - Context-aware snippets (if, unless, while, def, class, module, each, map, etc.)
  - Deduplication of completions

- **Document Symbols**

  - Nested symbol hierarchy
  - Classes, modules, methods, constants
  - Method visibility (public/private/protected)
  - Method kind (class/instance)

- **Workspace Symbols**

  - Fuzzy search across all indexed symbols
  - Camel case matching (e.g., "AppCtrl" → "ApplicationController")
  - Subsequence matching

- **Inlay Hints**

  - End keyword hints for class/module/method blocks

- **Code Folding**

  - Classes, modules, methods
  - Control flow (if, while, case, begin)
  - Multi-line arrays and hashes
  - Blocks

- **Diagnostics**

  - Syntax errors from ruby-prism parser
  - Parser warnings

- **Code Lens**

  - Module mixin usage counts (include/prepend/extend)
  - Class inheritance tracking

- **On-Type Formatting**
  - Automatic `end` keyword insertion

### Planned Features

- Hover information
- Code actions / Quick fixes
- Rename support
- Formatting integration (Rubocop)
- Full method reference support
- Instance/class/global variable support
- Type inference
- Meta-programming support
- Run/Debug support

## Architecture Philosophy

The project follows a modular architecture with clear separation of concerns:

- **Indexer**: Tracks symbol locations across the workspace

  - Supports project files, stdlib stubs, and gem dependencies
  - Uses fully qualified names (FQN) without artificial prefixes
  - Maintains method-by-name index for quick lookups

- **Analyzer**: Understands Ruby code structure and semantics using Prism parser

  - Visitor-based AST traversal
  - Scope tracking for namespace resolution
  - Identifier resolution at cursor positions

- **Capabilities**: Implements specific LSP features by combining indexer and analyzer

  - Each capability is self-contained
  - Shares common infrastructure for position handling

- **Server**: Coordinates LSP protocol handling and delegates to capabilities
  - Parent process monitoring for cleanup
  - Document caching with change tracking

## Target Users

Ruby developers using VS Code or other LSP-compatible editors who need fast, accurate code navigation and analysis across large Ruby codebases.
