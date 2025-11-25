# Ruby Fast LSP

A high-performance Ruby Language Server written in Rust, delivering fast and accurate code navigation, syntax highlighting, and intelligent code insights for Ruby developers across entire projects.

## Features

### Core Features

| Feature                 | Status | Details                                                       |
| ----------------------- | ------ | ------------------------------------------------------------- |
| **Syntax Highlighting** | ✅     | Full semantic token-based highlighting                        |
| **Workspace Indexing**  | ✅     | Project files, stdlib stubs, gem dependencies                 |
| **Go to Definition**    | ✅     | Classes, modules, constants, local variables, methods (basic) |
| **Find References**     | ✅     | Classes, modules, constants, local variables                  |
| **Code Completion**     | ✅     | Variables, constants, classes, modules, snippets              |
| **Document Symbols**    | ✅     | Nested hierarchy with visibility info                         |
| **Workspace Symbols**   | ✅     | Fuzzy search with camel case matching                         |
| **Inlay Hints**         | ✅     | End keyword hints for blocks                                  |
| **Code Folding**        | ✅     | Classes, modules, methods, control flow                       |
| **Diagnostics**         | ✅     | Syntax errors and warnings                                    |
| **Code Lens**           | ✅     | Module mixin usage counts                                     |
| **On-Type Formatting**  | ✅     | Auto-insert `end` keyword                                     |

### Navigation Details

- **Go to Definition**

  - Modules ✅
  - Classes ✅
  - Constants ✅
  - Local variables ✅
  - Methods (limited) 🚧
  - Instance/Class/Global variables 🚧

- **Find References**
  - Modules ✅
  - Classes ✅
  - Constants ✅
  - Local variables ✅
  - Methods (limited) 🚧

### Planned Features

- Hover information ❌
- Code actions / Quick fixes ❌
- Rename support ❌
- Formatting integration (Rubocop) ❌
- Full type inference ❌
- Meta-programming support ❌
- Run/Debug support ❌

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

- Method references may be incomplete in some cases
- Large workspaces may take time to index initially
- Some edge cases in Ruby syntax may not be fully supported yet

## Contributing

Please report any issues or feature requests on our [GitHub repository](https://github.com/rajnaveen344/ruby-fast-lsp).

## License

MIT
