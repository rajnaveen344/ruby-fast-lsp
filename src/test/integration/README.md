# Integration Tests

This directory contains integration tests for the Ruby Fast LSP server. Tests
are grouped by the same feature families as `src/lsp/capabilities/` and
`src/lsp/query/`, then by the Ruby behaviour under test.

## Structure

- `diagnostics/` - Diagnostic publication and kinds
  - `method_calls/` - Arity, keyword arguments, misspelled and unresolved methods
  - `flow/` - Unreachable code, inconsistent returns, nil receivers
  - `value_types/` - Bad splat targets and raised non-exceptions
  - `external_linter.rs` - External linter diagnostics and quick fixes
- `navigation/` - `goto/`, `references/`, `implementation/`, `call_hierarchy/`,
  `type_hierarchy/`, `document_highlights.rs`
- `editing/` - `completion/`, `rename/`, `formatting.rs`, `signature_help.rs`
- `lifecycle/` - Deterministic schedules around collection, commit, dependency
  refresh, and diagnostic publication; observer controls; exact results and
  method targets across edits, partial opens, and closed buffers
- `presentation/` - `hover/`, `inlay_hints/`, `code_lens/`, `folding_range/`,
  `selection_ranges.rs`
  - `inlay_hints/variable_type/assigned_values/` - Hints from literals, constants,
    constructors, and method calls
  - `inlay_hints/variable_type/local_flow/` - Reassignment, branches, rescue,
    diverging branches, and guard narrowing
- `semantics/` - Cross-cutting Ruby semantics
  - `inference/` - `method_resolution/`, `return_types/`, `declared_types/`,
    `callables/`
  - `constants/` - Constant values, collections, and YARD types
  - `mixins/` - Module inclusion
- `sources/` - Source kinds and project ownership: `erb.rs`, `extensions.rs`,
  `workspaces/`

Every folder holds at most ten immediate entries; see
`support/structure/README.md`.

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

1. Determine the feature family and the Ruby behaviour being tested.
2. Add your test file to the corresponding directory (e.g., `navigation/goto/methods/my_feature.rs`).
3. If creating a new directory, ensure it has a `mod.rs` and declare it in its parent `mod.rs`.
4. Use the harness check functions whenever possible instead of manual server setup.
