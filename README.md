# Ruby Fast LSP

A high-performance Ruby Language Server written in Rust, delivering fast and accurate code navigation, syntax highlighting, and intelligent code insights for Ruby developers across entire projects.

## Features

### Core Features (in no particular order)

- **Syntax Highlighting** - Accurate syntax highlighting for Ruby files
- **Workspace Indexing** - Automatic workspace indexing on initialization
  - Project ruby files 🎉
  - Gems/Gemfile ❗
- **Code Navigation** - Fast navigation to definitions and references
  - Go to definition 🎉
    - Modules 🎉
    - Classes 🎉
    - Constants 🎉
    - Methods (limited support) 🚧
    - Local variables 🎉
    - Class/Instance variables 🚧
    - Global variables 🚧
  - Find references 🎉
    - Modules 🎉
    - Classes 🎉
    - Constants 🎉
    - Methods (limited support) 🚧
    - Local variables 🎉
    - Class/Instance variables 🚧
    - Global variables 🚧
- **Code Completions** - Intelligent suggestions for:
  - Local variables 🎉
  - Method names and parameters ❗
  - Class and module names ❗
  - Snippets (class, module, def, do, while, until, if, unless) ❗
- **Inlay Hints** - Helpful inline hints for better code understanding
  - class/module/method "end" hints 🎉
  - method parameter hints ❗
- **Code Diagnostics (TODO)** - Code diagnostics for code warnings, errors and issues
- **Code Lens (TODO)** - Code lens for better contextual information
- **Run/Debug Support (TODO)**
- **Code Actions (TODO)**
- **Code Folding (TODO)**
- **Documents Symbol (TODO)** - Document symbols for document outline
- **Workspace Symbol (TODO)** - Workspace symbols for workspace wide constant and method navigation
- **Hover Information (TODO)**
- **Code Formatting (TODO)** - Automatic code formatting based of config (Rubocop, etc.)
- **Type Inference (TODO)**
- **Meta Programming Support (TODO)**

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

## Known Issues

- Method references may be incomplete in some cases
- Large workspaces may take time to index initially
- Some edge cases in Ruby syntax may not be fully supported yet

Please report any issues or feature requests on our [GitHub repository](https://github.com/rajnaveen344/ruby-fast-lsp).