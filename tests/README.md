# Tests Directory

This directory contains test files used for manual testing and debugging the Ruby Fast LSP server.

## Files

### test.rb

`test.rb` is a simple Ruby class file used for testing the LSP features such as:

- Goto Definition
- Find References
- Indexing
- Local variable resolution

The file contains:
- A `Person` class with instance variables and methods
- Local variable definitions and references
- Method calls that can be used to test navigation

Use this file in your editor when you want to test that LSP features like "goto definition" are working correctly.

## Usage

To test using this file:

1. Start the Ruby Fast LSP server:
   ```
   ./target/release/ruby-fast-lsp
   ```

2. Open the `test.rb` file in your editor that's configured to use the LSP server

3. Try features like:
   - Jump to definition on variables like `greeting`, `age_threshold`
   - Jump to definition on methods like `adult?`, `greet`
   - Find references to methods and variables
