# Rails Ruby extension

This bundled Ruby Fast LSP extension contributes static Rails semantics through
the public mruby extension SDK. It does not access server or engine internals.

The initial contract supports `belongs_to`, `has_one`, and `has_many`. Each
association contributes a public reader and writer, a structured return type,
and navigation from the association name to its conventional target class.
Removing the declaration removes those facts during normal file reindexing.

Literal `class_name` options override the conventional target, including
namespaced constants, and navigation uses the exact option-value range.
Polymorphic `belongs_to` declarations still create readers and writers but
intentionally use an unknown type and do not invent a constant target.

Active Record lifecycle callbacks (`before_save`, `after_commit`, and the other
standard validation/save/create/update/destroy/initialize/find/touch variants)
and custom `validate :method_name` declarations navigate from literal symbol or
string arguments to instance methods. `validates` and the standard
`validates_*_of` helpers similarly treat positional attribute names as reader
method references. These references use normal engine MRO,
private-method, ambiguity, find-references, and edit-removal behavior. Proc and
block callbacks remain ordinary Ruby code and need no synthetic method target.

Routes inside `Rails.application.routes.draw` support `resources`, `resource`,
`root`, and named `get`/`post`/`put`/`patch`/`delete`/`match` declarations.
Resource and named routes contribute `_path` and `_url` methods to
`ApplicationController`, with `String` return types and permissive positional/
keyword signatures. Controller subclasses therefore navigate from helper calls
back to the route declaration. Conventional resource controller targets and
explicit `to: "users#show"` controller/action segments are semantic references.
Nested `namespace` and `scope module:/as:` frames prefix controller targets and
helper names; common irregular plurals such as `people` use explicit stable
singular forms.

Conventional target names use deterministic inflection: snake case is
camelized and collection names use a small singularization rule. Options such
as `through`, `source`, custom foreign keys, conditional validation semantics,
validator classes, and dynamic `class_name` values are not inferred yet.
Route helpers are currently projected into controller inheritance, not view/
template contexts. Routes with `only` or `except` conservatively emit controller
navigation but no helper set; `member`/`collection`, shallow routes, mounted
engines, and the full Active Support inflector remain future route work.

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
