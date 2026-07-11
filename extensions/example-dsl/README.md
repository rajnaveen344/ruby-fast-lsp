# Example DSL extension

This independent example is the acceptance fixture and copyable starting point
for the public mruby extension SDK. It demonstrates all of the minimum package
contracts without importing Ruby Fast LSP server or engine internals:

- `field :name` emits a private instance-method semantic patch with a nilable
  `Array<String>` return type;
- the same declaration emits a generated `GeneratedRecord` class and typed
  `GeneratedRecord::DEFAULT_NAME` constant with a `Hash<Symbol, String>` type
  through public semantic patches;
- `GeneratedRecord` inherits `BaseRecord` through the public `set_superclass`
  relationship patch, and inherited method lookup uses the ordinary engine MRO;
- fields appear as document symbols;
- fields receive an editor code lens;
- the generated namespace, constant, and method participate in ordinary
  engine-owned navigation and type hover, and disappear on reindex;
- the `field` argument is an engine-owned semantic reference to the generated
  class, so find-references includes the DSL declaration and removes it after
  an edit.

Run the source-level SDK test:

```bash
ruby -I extensions/mruby-sdk extensions/example-dsl/test/example_dsl_test.rb
```

Run the black-box LSP acceptance test:

```bash
cargo test -p ruby-fast-lsp-test-harness --test third_party_extension
```

`contract.wat.in` and the JSON files are a deterministic public-ABI fixture
compiled by the black-box test. They keep the acceptance test independent of a
locally installed mruby/WASI toolchain. Production packages ship the Wasm built
from `extension.rb` and `runtime.rb`.

Build and validate this package with the reusable SDK toolchain:

```bash
extensions/mruby-sdk/scripts/build-wasm-docker.sh extensions/example-dsl
cargo run --bin extension -- validate extensions/example-dsl
RUBY_FAST_LSP_TEST_BUILT_EXAMPLE=1 \
  cargo test -p ruby-fast-lsp-test-harness --test third_party_extension
```
