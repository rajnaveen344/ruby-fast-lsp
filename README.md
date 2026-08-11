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

See [Next Engineering Goals](NEXT.md) for the forward-looking inference and analysis roadmap.

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
        ".gemspec": "ruby",
        ".ru": "ruby",
        ".thor": "ruby",
        ".jbuilder": "ruby",
        ".rbi": "ruby",
        ".erb": "erb",
        ".rhtml": "erb"
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

The bottom-right Ruby runtime item follows the owning project of the active
Ruby file. Click it to select an exact runtime, switch that project to Auto,
inspect all project runtimes, or select the linter and formatter. Selections are
stored privately per VS Code workspace and never require `settings.json`.

**Save Runtime to .ruby-version** writes the exact effective runtime into the
active Ruby project after confirmation and switches that project to Auto. This
is the repository-persistent option for teams and multi-project workspaces. Auto
resolves exact `.ruby-version` and `.tool-versions` identities against the
server's bounded installed-runtime catalog; if the exact installation is
missing or ambiguous, Ruby Fast LSP does not substitute a nearby version.

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
saved. They never run in the `didChange` typing path. In VS Code, run
**Ruby Fast LSP: Select Linter** and choose RuboCop, Standard, or Disabled.

By default the server executes `bundle exec rubocop` or
`bundle exec standardrb` in the workspace root and sends the current editor
buffer over stdin. The owning project's Bundler environment is the only VS Code
execution path; the extension does not expose arbitrary command configuration.

Linter failures and timeouts are logged without replacing syntax or semantic
diagnostics. A successful lint report is merged with Ruby Fast LSP diagnostics.
Correctable offenses offer a preferred `Quick Fix` that runs RuboCop's safe
`--autocorrect` mode or Standard's safe `--fix` mode against the current buffer
and returns an editor-applied document edit. Unsafe RuboCop corrections are not
applied.

## Full-document formatting

VS Code can format the current unsaved Ruby buffer with RuboCop's safe
`--autocorrect` mode or Standard's `--fix` mode. Formatting is opt-in and
independent from lint diagnostics. Run **Ruby Fast LSP: Select Formatter** and
choose RuboCop, Standard, or Disabled. The server runs `bundle exec rubocop` or
`bundle exec standardrb` in the owning project. Commands receive the current
buffer through stdin. Unchanged output produces no edit; startup failures,
timeouts, invalid UTF-8, empty output for a non-empty source, and abnormal
exits are reported without changing the document or semantic index.

ERB templates are not sent to Ruby formatters or linters. HTML formatting is
also intentionally disabled because whole-document edits could overwrite
embedded Ruby; HTML validation is not yet delegated from the host projection.

## ERB templates

The VS Code extension activates for `.erb` files and analyzes Ruby inside
`<% ... %>` and `<%= ... %>` regions. Definition, references, hover,
completion, diagnostics, symbols, semantic tokens, selection and folding
ranges, inlay hints, rename analysis, and code lenses use an offset-preserving
Ruby projection, so results retain their original template UTF-16 positions
even when surrounding HTML contains multibyte text.

ERB comments (`<%# ... %>`), escaped tags (`<%% ... %>`), and host-language
text are excluded from Ruby analysis. Unclosed tags are masked rather than
guessed. In VS Code, an offset-stable complementary HTML projection provides
host-language completion, hover, document symbols, folding, selection ranges,
and document highlights. Ruby regions are blanked one UTF-16 code unit at a
time, so HTML results cannot target embedded Ruby. Formatting, diagnostics,
links, colors, and rename are not currently delegated to the HTML service.

## Project indexing

Project indexing is automatic and deterministic. Standard Ruby files are
indexed by default, default-external trees are excluded, and `.git` is never
traversed. Dependencies come from the owning project's exact Bundler context.

Workspace folders are Ruby project containers. A root `Gemfile` owns the whole
folder. Without one, Ruby Fast LSP discovers the nearest nested Gemfiles, stops
below each discovered root, and gives every project an isolated semantic
engine, Bundler environment, diagnostics state, and extension fact lifecycle.
Git repositories do not define semantic boundaries; `.git` is only pruned from
discovery. Open files outside discovered projects remain available through the
orphan engine.

Source ownership is explicit:

- Workspace files selected by the project policy are editable project sources.
  They contribute references and diagnostics and appear in workspace-symbol
  search.
- `vendor`, `.bundle`, `.ruby-lsp`, `.ruby-fast-lsp`, `node_modules`, `tmp`,
  `log`, and `coverage` trees are external. Opening one of these files provides
  interactive semantic analysis but
  does not silently promote it into workspace symbols, rename edits, external
  linting, or project semantic diagnostics; edits retain that ownership and
  closing the file removes its interactive-only facts.
- Generated Ruby in an ordinary project path is treated as project source
  because path names cannot reliably prove ownership. Extension-generated
  declarations inherit the ownership of their source file and are removed
  through that file's reindex lifecycle.
- Bundler/RubyGems dependencies, stdlib, and bundled stubs remain available for
  navigation and inference but do not contribute project diagnostics,
  workspace-symbol results, or rename edits.
- Extracted Git dependencies under `vendor/cache/<repository>-<revision>` are
  read from the owning project's `Gemfile.lock` when Bundler's normal checkout
  is unavailable. They enter only that project's engine as dependency sources;
  their gemspecs are never executed by the fallback.
- `.git` is never traversed or opt-in indexable. Workspace trust controls local
  Wasm extensions, not ordinary static Ruby parsing.

Native and generated declarations have three supported static paths:

- Ruby and RBI declaration files are indexed as ordinary Ruby. This is the
  preferred path when a generator can emit navigable method bodies or RBI
  stubs.
- RBS files under `sig/` are discovered automatically. RBS classes, modules,
  methods, constructors, attributes, constants, visibility,
  mixins, inheritance, parameter labels, and structured return types become
  engine-owned signature facts. Calls can navigate into a signature and use it
  for signature help and hover. When matching Ruby implementation facts exist,
  navigation prefers the implementation while RBS remains its type/signature
  overlay. RBS files never produce Ruby diagnostics, rename edits, or project
  workspace-symbol entries.
- APIs generated by DSLs or optional runtime knowledge use the sandboxed public
  extension patch contracts. Runtime helpers may request ordinary file reindex;
  they cannot write semantic state directly.

Binary native extensions without Ruby, RBI, RBS, or an extension package cannot
be reconstructed safely from the binary alone. Generate or check in RBS/RBI, or
ship a Ruby Fast LSP extension. RBS interfaces, type aliases, and method aliases
are currently parsed but do not yet become navigation declarations.

The source policy covers the common Ruby entry points advertised by
Shopify Ruby LSP: `.rb`, `.rake`, `.gemspec`, `.ru`, `.thor`, `.jbuilder`,
`.rbi`, `.podspec`, and related Ruby DSL extensions; conventional files such as
`Gemfile`, `Rakefile`, `Thorfile`, `Fastfile`, `Dangerfile`, `Podfile`, and
`.simplecov`; and `.erb`, `.rhtml`, and `.rhtm` templates. VS Code language
association, filesystem watchers, and server discovery are checked against the
canonical list in `editors/vscode/vsix/ruby_file_kinds.json`. For closed files,
watcher create/change events replace facts through the normal engine write path
and delete events remove stale facts; open buffers remain owned by the document
lifecycle.

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
