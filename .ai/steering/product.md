# Ruby Fast LSP

Ruby Fast LSP is a high-performance Language Server Protocol (LSP) implementation for Ruby, written in Rust. It provides fast and accurate code navigation, syntax highlighting, and intelligent code insights for Ruby developers across entire projects.

## Core Features

- **Workspace Indexing**: Automatic indexing of Ruby files and symbols
- **Code Navigation**: Go-to-definition and find-references for classes, modules, constants, methods, and variables
- **Syntax Highlighting**: Semantic token-based syntax highlighting
- **Code Completion**: Intelligent suggestions for local variables and symbols
- **Inlay Hints**: Contextual hints for method parameters and block endings

## Architecture Philosophy

The project follows a modular architecture with clear separation of concerns:
- **Indexer**: Tracks symbol locations across the workspace
- **Analyzer**: Understands Ruby code structure and semantics using Prism parser
- **Capabilities**: Implements specific LSP features by combining indexer and analyzer
- **Server**: Coordinates LSP protocol handling and delegates to capabilities

## Target Users

Ruby developers using VS Code or other LSP-compatible editors who need fast, accurate code navigation and analysis across large Ruby codebases.
