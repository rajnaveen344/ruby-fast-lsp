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

Keep this in your user-level Claude Code settings. This repository does not carry project-local `.claude` configuration.

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

## RuboCop and Standard diagnostics

External lint diagnostics are opt-in and run when a Ruby document is opened or
saved. They never run in the `didChange` typing path. In VS Code, set
`rubyFastLsp.linter` to `rubocop` or `standard`.

By default the server executes `bundle exec rubocop` or
`bundle exec standardrb` in the workspace root and sends the current editor
buffer over stdin. To use a binstub or another Ruby environment, configure a
structured argv array—no shell parsing is performed:

```json
{
  "rubyFastLsp.linter": "rubocop",
  "rubyFastLsp.linterCommand": ["bin/rubocop"]
}
```

Linter failures and timeouts are logged without replacing syntax or semantic
diagnostics. A successful lint report is merged with Ruby Fast LSP diagnostics.
Correctable offenses offer a preferred `Quick Fix` that runs RuboCop's safe
`--autocorrect` mode or Standard's safe `--fix` mode against the current buffer
and returns an editor-applied document edit. Unsafe RuboCop corrections are not
applied.

## Full-document formatting

VS Code can format the current unsaved Ruby buffer with RuboCop's safe
`--autocorrect` mode or Standard's `--fix` mode. Formatting is opt-in and
independent from lint diagnostics:

```json
{
  "rubyFastLsp.formatter": "standard",
  "rubyFastLsp.formatterCommand": ["bin/standardrb"]
}
```

When `formatterCommand` is empty, the server runs `bundle exec rubocop` or
`bundle exec standardrb` in the workspace root. Commands receive the current
buffer through stdin. Unchanged output produces no edit; startup failures,
timeouts, invalid UTF-8, empty output for a non-empty source, and abnormal exits
are reported without changing the document or semantic index.

ERB templates are not sent to Ruby formatters or linters. Ruby Fast LSP masks
the host-language bytes for semantic analysis, but it does not yet delegate
HTML formatting or validation to another language server.

## ERB templates

The VS Code extension activates for `.erb` files and analyzes Ruby inside
`<% ... %>` and `<%= ... %>` regions. Definition, references, hover,
completion, diagnostics, symbols, semantic tokens, selection and folding
ranges, inlay hints, rename analysis, and code lenses use an offset-preserving
Ruby projection, so results retain their original template UTF-16 positions
even when surrounding HTML contains multibyte text.

ERB comments (`<%# ... %>`), escaped tags (`<%% ... %>`), and host-language
text are excluded from Ruby analysis. Unclosed tags are masked rather than
guessed. HTML language-server delegation is not currently provided, so Ruby
Fast LSP complements rather than replaces HTML tooling inside template files.

## Indexing configuration

Project indexing accepts workspace-relative glob patterns and explicit gem
selection. Standard Ruby files are indexed by default. `includedPatterns` can
add nonstandard Ruby entry points such as `bin/console`; `excludedPatterns`
always win, and `.git` is never traversed. Explicitly included gems augment
dependencies inferred from source, while excluded gems are omitted even when
they are direct or transitive dependencies.

```json
{
  "rubyFastLsp.indexing": {
    "includedPatterns": ["bin/*"],
    "excludedPatterns": ["vendor/**/*", "tmp/**/*.rb"],
    "includedGems": ["rails"],
    "excludedGems": ["debug"]
  }
}
```

The VS Code extension restarts the language server when this setting changes so
that excluded files cannot leave stale semantic facts behind. Other LSP clients
should send the same `indexing` object in initialization options and restart the
server after changing it. Invalid glob patterns abort workspace indexing with an
actionable error instead of silently applying a partial policy.

## Test discovery and execution

The packaged VS Code extension discovers RSpec and Minitest declarations as
document symbols and adds Run and Debug code lenses. RSpec targets run through
`bundle exec rspec file:line`. Minitest uses `bin/rails test file:line` when a
Rails runner is present, otherwise it uses `bundle exec ruby -Itest` with an
exact method filter. Debug lenses launch an `rdbg` configuration and require the
`debug` gem plus a compatible VS Code Ruby debugger extension.

Test commands use structured process argv without an implicit shell and reject
malformed or non-file targets.
Minitest discovery is intentionally limited to conventional `test/` or
`*_test.rb` files, `*Test` classes, `def test_*`, and Rails-style `test "…"`
declarations.

Public controller actions in conventional `app/controllers/**/*_controller.rb`
files receive an **Open View** code lens. VS Code opens the first existing
conventional template in this order: `.html.erb`, `.turbo_stream.erb`, then
`.json.jbuilder`. Private/protected methods and paths outside the controller
convention do not receive this lens.

## Project-local extensions

Trusted workspaces can discover manifest packages automatically from
`.ruby-fast-lsp/extensions/*/extension.toml` and
`ruby_fast_lsp/**/extension.toml`. VS Code passes its Restricted Mode trust
state to the server; project-local Wasm never loads from an untrusted workspace.
Set `rubyFastLsp.projectExtensionsEnabled` to `false` to disable automatic local
discovery even in trusted workspaces.

Configured and bundled `extensionPackages`/`extensionDirs` have highest
precedence, followed by project-local packages and then environment/development
paths. Duplicate IDs within one source use filesystem-path order, making
multi-root activation deterministic. Every package still passes manifest,
compatibility, checksum, permission, and Wasm resource validation.

## See Also

- [Ruby Fast Cop](https://github.com/rajnaveen344/ruby-fast-cop) - A high-performance Ruby linter written in Rust, designed as a companion to Ruby Fast LSP.

## Contributing

Issues and feature requests welcome on [GitHub](https://github.com/rajnaveen344/ruby-fast-lsp).

## License

MIT
