# Rails Ruby extension

This bundled Ruby Fast LSP extension contributes static Rails semantics through
the public mruby extension SDK. It does not access server or engine internals.

The initial contract supports `belongs_to`, `has_one`, and `has_many`. Each
association contributes a public reader and writer, a structured return type,
and navigation from the association name to its conventional target class.
Removing the declaration removes those facts during normal file reindexing.

Target names currently use deterministic convention-only inflection: snake
case is camelized and collection names use a small singularization rule. Rails
options such as `class_name`, `through`, `source`, `polymorphic`, and custom
foreign keys are intentionally not inferred yet.

Run the source contract tests with:

```bash
ruby -Iextensions/mruby-sdk extensions/rails-ruby/test/rails_ruby_test.rb
```

Build the Wasm guest with the shared SDK builder:

```bash
extensions/mruby-sdk/scripts/build-wasm-docker.sh extensions/rails-ruby
```

Then exercise that artifact through the black-box LSP test with:

```bash
RUBY_FAST_LSP_TEST_BUILT_RAILS=1 \
  cargo test -p ruby-fast-lsp-test-harness --test rails_extension
```
