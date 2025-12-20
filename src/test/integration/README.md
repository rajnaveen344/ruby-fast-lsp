# Integration Tests

This directory contains integration tests for the Ruby Fast LSP server. Tests are organized by the **tested entity**.

## Structure

- `classes/` - Tests for class-related features (goto definition, references)
- `constants/` - Tests for constants (goto definition, references)
- `methods/` - Tests for methods (goto definition, chaining, inlay hints)
- `modules/` - Tests for modules (code lens, mixins)
- `variables/` - Tests for variables, organized by scope:
  - `class/`
  - `global/`
  - `instance/`
  - `local/`

## Test Harness

We provide a comprehensive test harness in `src/test/harness` that supports marker-based testing similar to rust-analyzer.

### Markers

- `$0` - Cursor position (for goto definition, type inference, etc.)
- `<def>...</def>` - Expected definition range (for goto definition)
- `<ref>...</ref>` - Expected reference ranges (for find references)
- `<lint:CODE>...</lint:CODE>` - Expected diagnostics (e.g., `<lint:err>`)
- `<hint:TYPE>...</hint:TYPE>` - Expected inlay hint location (e.g., `<hint:String>`)
- `<lens:COMMAND>...</lens:COMMAND>` - Expected code lens location

### Available Check Functions

All check functions accept a Ruby source string with markers.

| Function            | Usage                  | Markers Used                    |
| ------------------- | ---------------------- | ------------------------------- |
| `check_goto`        | Verify goto definition | `$0` (cursor), `<def>` (target) |
| `check_references`  | Verify find references | `$0` (cursor), `<ref>` (refs)   |
| `check_inlay_hints` | Verify type hints      | `<hint:TYPE>`                   |
| `check_code_lens`   | Verify code lenses     | `<lens:COMMAND>`                |
| `check_diagnostics` | Verify diagnostics     | `<lint:SEVERITY>`               |

### Example

```rust
use crate::test::harness::check_goto;

#[tokio::test]
async fn test_goto_class() {
    check_goto(r#"
<def>class Foo</def>
end

Foo$0.new
"#).await;
}
```

## Adding New Tests

1. Determine the primary entity being tested (Class, Method, Variable, Module, etc.).
2. Add your test file to the corresponding directory (e.g., `methods/my_feature.rs`).
3. If creating a new directory, ensure it has a `mod.rs` and update `src/test/integration/mod.rs`.
4. Use the harness check functions whenever possible instead of manual server setup.
