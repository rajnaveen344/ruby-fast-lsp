# Ruby Fast LSP

A high-performance Ruby Language Server written in Rust, built to power both AI coding agents and traditional editors with fast, accurate code intelligence.

## Why Ruby Fast LSP

AI coding agents like Claude Code, Cursor, and Windsurf rely on Language Server Protocol features to understand codebases, validate their own edits, and navigate code with precision. Ruby deserves a language server that treats these agent workflows as first-class.

Ruby Fast LSP is designed around the features that matter most for agent-assisted development:

- **Diagnostics** that catch errors in real time, so agents can self-correct without running a build
- **Go to Definition** and **Find References** for precise, type-aware navigation instead of text search
- **Hover** with type signatures, so agents understand what they're working with
- **Workspace Symbols** for systematic codebase exploration

Written in Rust with millisecond response times, it handles large Ruby codebases without becoming a bottleneck.

## Type Inference

At the core of Ruby Fast LSP is Yard & RBS backed type inference engine that gives diagnostics and navigation real accuracy, not just syntax awareness.

- Resolves standard library types through RBS definitions
- Handles generic substitution (e.g., `Array[Integer]#first` resolves to `Integer`)
- Walks ancestor chains across includes, prepends, and inheritance
- Validates return types against YARD and RBS annotations
- Understands union types (eg. `User, nil`) for accurate nullability and branch analysis
- Powers unresolved method and constant detection

You can guide the engine with simple YARD annotations on your methods:

```ruby
# @param name [String]
# @return [User, nil]
def find_by_name(name)
  # ...
end
```

This is enough for the LSP to resolve return types, validate callers, and propagate types through method chains. No separate type files or complex setup required.

This is what makes the difference between a language server that can grep and one that can reason about Ruby code.

## Installation

### npm (recommended)

Install the language server binary globally:

```bash
npm install -g @ruby-fast/lsp
```

This makes the `ruby-fast-lsp` binary available in your PATH.

### Building from Source

```bash
cargo build --release
```

The binary will be at `target/release/ruby-fast-lsp`.

## Setup

### Claude Code

1. Install the binary globally via npm (see above).

2. Add the language server to your Claude Code settings. Edit `~/.claude/settings.json`:

```json
{
  "lspServers": {
    "ruby": {
      "command": "ruby-fast-lsp",
      "args": ["--stdio"],
      "extensionToLanguage": {
        ".rb": "ruby",
        ".rake": "ruby",
        ".gemspec": "ruby"
      }
    }
  }
}
```

3. Restart Claude Code. The language server will start automatically when you work with Ruby files, providing diagnostics, navigation, and type information.

For project-specific configuration, add the same `lspServers` block to `.claude/settings.json` in your project root instead.

### VS Code

1. Install the extension from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=naveenraj.ruby-fast-lsp).
2. Open a Ruby project. The server starts automatically and indexes your workspace.

### Cursor, Windsurf, and Other VS Code Forks

Editors based on VS Code that use the [Open VSX Registry](https://open-vsx.org/) can install the extension from:

[open-vsx.org/extension/naveenraj/ruby-fast-lsp](https://open-vsx.org/extension/naveenraj/ruby-fast-lsp)

### Other Editors

Any editor that supports LSP can use Ruby Fast LSP. Start the server with:

```bash
ruby-fast-lsp --stdio
```

Configure your editor's LSP client to connect via stdio with language ID `ruby`.

## See Also

- [Ruby Fast Cop](https://github.com/rajnaveen344/ruby-fast-cop) - A high-performance Ruby linter written in Rust, designed as a companion to Ruby Fast LSP.

## Contributing

Issues and feature requests welcome on [GitHub](https://github.com/rajnaveen344/ruby-fast-lsp).

## License

MIT
