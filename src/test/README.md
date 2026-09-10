# Tests Directory

This directory contains test files used for testing the Ruby Fast LSP server.

## Structure

- **fixtures/**: Contains Ruby files used for testing the parser, indexer, and LSP features
- **integration_test.rs**: Integration tests for verifying fixture availability and basic integration points

## Fixtures

The `fixtures/` directory contains various Ruby files that test different aspects of the Ruby language and LSP functionality. These fixtures include:

- Class and module definitions
- Method declarations and calls
- Variable declarations (local, instance, class)
- Control flow structures
- Error handling
- Blocks and procs

Additionally, there are LSP-specific fixtures for testing:

- Definition/goto functionality
- References
- Symbols
- Completion
- Hover information

## Integration Tests

The integration tests focus on ensuring that all LSP related functionalities(goto, references, completion, etc.) is working as expected for all fixtures.

## Running Tests

Run all tests with:

```bash
cargo test
```

Run only the integration tests with:

```bash
cargo test --test integration_test
```

## Adding Tests

When adding new tests:

1. Add new Ruby fixtures to the `fixtures/` directory
2. Add test functions to the appropriate test file

### Adding LSP Integration Tests

For future LSP-specific integration tests:

1. Focus on testing the integration between our Ruby indexing/parsing and the LSP protocol
2. Avoid duplicating tests already covered by the `tower_lsp` crate
3. Use the provided fixtures to test specific LSP features like definition, references, etc.

## Test Harness Helpers

We provide a unified `check()` function in `src/test/harness` that auto-detects what to verify based on inline markers.

### Unified Check Function (Recommended)

Use `check()` for all LSP tests - it determines what to verify from the tags present:

```rust
use crate::test::harness::check;

// Goto definition: $0 cursor + <def> tags
check(r#"
<def>class Foo
end</def>

Foo$0.new
"#).await;

// Inlay hints: <hint> tags
check(r#"x<hint label="String"> = "hello""#).await;

// Diagnostics: <err>/<warn> tags
check(r#"class <err>end</err>"#).await;

// Code lens: <lens> tags
check(r#"
module MyModule <lens title="include">
end

class MyClass
  include MyModule
end
"#).await;

// References: $0 cursor + <ref> tags
check(r#"
class <ref>Foo$0</ref>
end

<ref>Foo</ref>.new
"#).await;
```

### Supported Markers

| Tag                  | Requires `$0` | Purpose                        |
| -------------------- | ------------- | ------------------------------ |
| `<def>...</def>`     | Yes           | Expected goto definition range |
| `<ref>...</ref>`     | Yes           | Expected reference range       |
| `<type>...</type>`   | Yes           | Expected type at cursor        |
| `<hint label="...">` | No            | Expected inlay hint            |
| `<lens title="...">` | No            | Expected code lens             |
| `<err>...</err>`     | No            | Expected error diagnostic      |
| `<warn>...</warn>`   | No            | Expected warning diagnostic    |

### The `none` Attribute (Range-Scoped)

Use `none` to assert zero occurrences **within the wrapped range**:

```rust
// No errors expected in this block
check(r#"<err none>class Foo; end</err>"#).await;

// No warnings expected in this block
check(r#"<warn none>def bar; end</warn>"#).await;

// No inlay hints expected in this block
check(r#"<hint none>FOO = 42</hint>"#).await;

// No code lenses expected in this block
check(r#"<lens none>module Unused; end</lens>"#).await;
```

**Important**: The `none` attribute requires a closing tag (e.g., `</err>`). The assertion only applies to the wrapped range, allowing you to have both positive and negative assertions in the same fixture.

### Lifecycle and publication observations

`FakeEditor` initializes a real `LspService`; its `client_messages` reader consumes
the ordinary outbound `ClientSocket`. Document operations and queries call the
production handlers directly. `diagnostics().await` waits for the submitted
diagnostics to arrive through `textDocument/publishDiagnostics`, including empty
clears. The observer never parses, collects facts, replaces sources, or resolves
the engine to repair an assertion.

`published_diagnostics()` and inline diagnostic tags observe synchronous
submission, which allows race tests to assert intermediate state before releasing
paused production work. Submission and delivered notifications are separate
evidence; a missing publication is not an empty clear. `with_cache_root` supplies
a fixture cache directory through ordinary server construction.

Deterministic gates remain at real indexing/commit/publication boundaries, with
worker progress observations owned by indexing. Ownership and publisher controls
live in `src/server/tests.rs`; simulation schedules live in `simulation/`.
The external `crates/lsp-test-harness` remains separate because it depends on the
root crate. Installed editor, process restart, and full inbound transport behavior
require their corresponding external or black-box tests.
