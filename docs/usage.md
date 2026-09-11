# Usage and support boundaries

See the [README](../README.md#get-started) for npm, VS Code/Open VSX, and source
installation. Other LSP clients start `ruby-fast-lsp --stdio` with language ID
`ruby`; use the client's current documentation for its configuration format.
Installing the npm server does not replace the server bundled in the VS Code
extension.

## Runtime and project indexing

In VS Code, click the Ruby runtime item or run **Ruby Fast LSP: Select Runtime**.
Selection follows the active file's owning project and is stored privately in
the editor workspace. Auto resolves exact project runtime markers. **Save Runtime
to .ruby-version** is a separate confirmed project write and switches to Auto.
Only `rubyFastLsp.logLevel` is exposed in the Settings page.

A root Gemfile owns its workspace folder. Without one, the server discovers the
nearest nested Gemfiles and isolates their engines, runtimes, dependencies, and
extension facts. A standalone Ruby folder without a Gemfile still receives core
analysis; it does not automatically search unrelated globally installed gems.
Open buffers take precedence over files on disk during background indexing.

Ordinary project files contribute editable references, diagnostics, and symbols.
Dependencies, stdlib, signatures, and default-external trees such as `vendor` or
`node_modules` remain navigation inputs without becoming editable project truth.
Project RBS under `sig/` supplies signatures; Ruby implementations take navigation
precedence when both exist. Native APIs need Ruby/RBI/RBS declarations or a
supported extension rather than inference from arbitrary binary code.

MRI runtime selection, JRuby compatibility/Java navigation, and TruffleRuby
runtime detection are implemented. Missing runtime discovery retains conservative
bundled Ruby 3.0 core stubs and skips runtime-dependent stdlib discovery. Recognized
runtime identities do not certify every Ruby/JDK/Bundler combination. Platform
packages and candidate validation are described in the [release checklist](development/release.md).

## Linting, fixes, and formatting

Use **Ruby Fast LSP: Select Linter** and **Ruby Fast LSP: Select Formatter**
independently to opt into RuboCop, Standard, or Disabled. The tools run through
the owning project's Bundler environment against the current unsaved buffer.
Linting runs on open/save, outside the typing path. Correctable diagnostics offer
safe quick fixes; formatting uses RuboCop `--autocorrect` or Standard `--fix`.
Failed or invalid tool output never becomes a document edit.

## ERB templates

Ruby inside ERB is analyzed using an offset-preserving projection, including
navigation, hover, completion, diagnostics, and rename. Original UTF-16 positions
are retained. Comments, escaped tags, host text, and unclosed tags do not become
Ruby source. `.erb`, `.rhtml`, and `.rhtm` are recognized template forms.

VS Code provides complementary HTML completion, hover, symbols, folding,
selection, and highlights. HTML diagnostics and whole-document edits are not
delegated; Ruby formatters and linters do not edit templates.

## Frameworks and test lenses

Bundled extensions activate from the owning project's complete lockfile. Their
manifests define the accepted dependency ranges and extension capabilities:
[Rails](../extensions/rails-ruby/extension.toml),
[RSpec](../extensions/rspec-ruby/extension.toml),
[Minitest](../extensions/minitest-ruby/extension.toml),
[Sinatra](../extensions/sinatra-rust/extension.toml), and
[Cucumber](../extensions/cucumber-rust/extension.toml).
Activation ranges are not an exhaustive framework compatibility guarantee.

RSpec and Minitest declarations offer Run/Debug lenses. RSpec uses
`bundle exec rspec file:line`; Minitest uses a Rails runner when available or an
exact method filter through Ruby. Debug requires the `debug` gem and a compatible
Ruby debugger extension. Rails controller actions can expose conventional Open
View targets. See the extension READMEs for supported DSL forms.

Trusted projects may supply manifest packages under
`.ruby-fast-lsp/extensions/*/extension.toml` or
`ruby_fast_lsp/**/extension.toml`. Project-local Wasm does not load in an untrusted
workspace. Packages still undergo compatibility, checksum, permission, and resource
validation. See [extensions](../extensions/README.md) for authoring.

## Inference limits

Incomplete evidence yields `Unknown`, with a reason when available. Runtime
reflection, string `eval`, unconstrained `method_missing`, unsupported mutation
or escape, and exceeded proof bounds do not justify concrete type guesses.
Hash shapes describe Hash-backed values, not arbitrary object properties.

Read the precise supported forms and limits for
[Hash shapes](features/structural-hash-shapes.md),
[higher-order calls](features/higher-order-call-inference.md), and
[callable bodies](features/callable-body-inference.md).
Navigation identity and ordering have their own
[contract](features/definition-navigation.md).

## Troubleshoot and report

Check **Ruby Fast LSP: Show Runtime Status**, **Show Project Indexing Status**,
and the **Ruby Fast LSP** output channel. Temporarily use `debug` logging for a
small reproduction, then return to `info`. Isolate overlapping Ruby language
providers when reproducing duplicate diagnostics or navigation.

Use the [bug report form](../.github/ISSUE_TEMPLATE/bug_report.yml). Include server
and editor-extension versions, OS/architecture, runtime, Bundler, project layout,
expected/actual behavior, whether indexing finished, and steps to reproduce.
Share generic reduced examples and sanitized logs; private source is unnecessary.
For simulation failures, retain the seed and replay artifact.

To roll back npm, install an explicitly selected previous version with
`npm install -g @ruby-fast/lsp@<previous-version>` and restart the client. In
VS Code, install the saved previous VSIX or use **Install Another Version...**
when available, then reload the window. Server SemVer and published extension
CalVer may differ; record both when comparing builds.
