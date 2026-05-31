# RSpec Ruby Extension

Ruby-authored prototype of the RSpec extension. The source is written against
`extensions/mruby-sdk` and is intended to be packaged into an mruby Wasm
module.

Current coverage:

- `let(:name) { ... }` -> generated instance helper method
- `let!(:name) { ... }` -> generated instance helper method
- `subject(:name) { ... }` -> generated instance helper method
- `subject { ... }` -> generated `subject` helper method
- Document symbols for `describe`, `context`, `it`, `example`, `specify`,
  `shared_examples`, and `shared_context`
- Code lenses for RSpec run/debug entry points

Local Ruby verification:

```bash
ruby -I extensions/mruby-sdk extensions/rspec-ruby/test/rspec_ruby_test.rb
```

## Wasm Build

Docker build:

```bash
extensions/rspec-ruby/scripts/build-wasm-docker.sh
```

Output:

```bash
extensions/rspec-ruby/target/wasm32-wasip1/release/rspec-ruby.wasm
```

Direct build expects a local mruby checkout and wasi-sdk:

```bash
MRUBY_ROOT=/path/to/mruby \
WASI_SDK_PATH=/path/to/wasi-sdk \
extensions/rspec-ruby/scripts/build-wasm.sh
```

The build patches mruby for trap-only Wasm exceptions. Ruby raises inside the
extension abort the guest; valid macro calls return JSON patches.

## Verification

```bash
cargo test -p ruby-fast-lsp-extension-wasm-host -- --nocapture
cargo test -p ruby-fast-lsp-test-harness -- --nocapture
RUBY_FAST_LSP_EXTENSION_PATHS="$PWD/extensions/rspec-ruby" \
  cargo test extensions -- --nocapture
```

## Loading

The extension is a server-loaded package, not a separate VS Code extension:

```bash
RUBY_FAST_LSP_EXTENSION_PATHS="$PWD/extensions/rspec-ruby" \
  ruby-fast-lsp
```

VS Code/Zed should pass this package path to the LSP server via initialization
options:

```json
{
  "extensionPackages": ["/path/to/rspec-ruby"],
  "extensionDirs": []
}
```

The editor wrapper can bundle `.wasm` and `extension.toml`, but Ruby Fast LSP
owns discovery, ABI validation, sandboxing, and index patch application.
