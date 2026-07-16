# Minitest Ruby extension

This bundled extension discovers conventional Minitest classes, `def test_*`
methods, and Rails-style `test "description"` declarations in `test/` and
`*_test.rb` files. It contributes document symbols plus Run and Debug code
lenses through the public typed Rust guest SDK and the same bounded Wasm ABI
available to external extensions. The stable package ID remains
`minitest-ruby` for compatibility with existing settings and editor commands.

Minitest::Spec `describe` groups create isolated hidden subclasses of
`Minitest::Spec`; nested groups inherit their parent without leaking into
siblings. Group bodies preserve lexical constants and locals while switching
the implicit receiver to the generated class and the `def` owner to its
instances. `it`, `specify`, `before`, `after`, `let`, and `subject` blocks run
against the owning group instance. `let` and `subject` emit ordinary generated
method facts with block-derived return types. Spec groups and examples also
receive symbols and Run/Debug lenses.

The package activates only when the owning isolated project has a complete
lockfile containing Minitest `>= 5, < 7`; unsupported or unknown versions fail
closed without disabling the installed guest.

Rails workspaces run exact `bin/rails test file:line` targets. Other workspaces
run Minitest through `bundle exec ruby -Itest file --name TEST_NAME`; a class
lens runs the complete file. Debug lenses launch the VS Code `rdbg` debugger and
therefore require the `debug` gem and a compatible Ruby debugger extension.

The extension deliberately ignores dynamically generated test names and files
outside conventional test paths.

Run the native guest tests, build and checksum the Wasm artifact, and run the
black-box LSP acceptance tests with:

```bash
extensions/minitest-ruby/build-and-test.sh
```
