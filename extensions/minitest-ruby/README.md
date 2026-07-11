# Minitest Ruby extension

This bundled extension discovers conventional Minitest classes, `def test_*`
methods, and Rails-style `test "description"` declarations in `test/` and
`*_test.rb` files. It contributes document symbols plus Run and Debug code
lenses through the public Ruby Fast LSP extension SDK.

Rails workspaces run exact `bin/rails test file:line` targets. Other workspaces
run Minitest through `bundle exec ruby -Itest file --name TEST_NAME`; a class
lens runs the complete file. Debug lenses launch the VS Code `rdbg` debugger and
therefore require the `debug` gem and a compatible Ruby debugger extension.

The extension deliberately ignores dynamically generated test names and files
outside conventional test paths.

Run source tests with:

```bash
ruby -Iextensions/mruby-sdk extensions/minitest-ruby/test/minitest_ruby_test.rb
```

Build the Wasm guest with:

```bash
extensions/mruby-sdk/scripts/build-wasm-docker.sh extensions/minitest-ruby
```
