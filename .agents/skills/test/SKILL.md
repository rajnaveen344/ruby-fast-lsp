---
name: test
description: "Write, debug, and understand Ruby Fast LSP tests, including check() tags, FakeEditor lifecycle tests, and black-box LSP tests."
---

# Testing

Use this skill when adding tests, debugging failures, or choosing the right harness.

## Commands

```bash
cargo test
cargo test test_name
cargo test -- --nocapture
cargo run --bin ast -- '<ruby snippet>'
cargo run --bin ast -- --loc '<ruby snippet>'
```

## Harness Choice

- Use `check()` for a single indexing pass with inline tags.
- Use `check_multi_file()` for cross-file scenarios that do not need edit lifecycle.
- Use `FakeEditor` for `didOpen`/`didChange`/`didSave`, multi-step editing, completion filtering, snippets, diagnostics lifecycle, and reindexing behavior.
- Use `crates/lsp-test-harness` only for black-box package/extension tests that must exercise public LSP initialization.

Do not merge `FakeEditor` and `crates/lsp-test-harness`; the external harness depends on the root crate and cannot be used by root crate internals without a cycle.

## Inline Tags

Common tags:

- `<complete items="a,b" excludes="c">`
- `<hint label="...">`
- `<def>...</def>`
- `<ref>...</ref>`
- `<type>...</type>`
- `<err>...</err>` and `<err none>...</err>`
- `<warn>...</warn>`
- `<lens title="...">`
- `<th supertypes="A,B" subtypes="C,D">`

Cursor tags use `$0`. LSP positions are 0-indexed; Prism offsets are byte offsets.

## TDD Rule For User Scenarios

When the user provides a concrete Ruby scenario:

1. Write the integration test first.
2. Run it and confirm it fails for the expected reason.
3. Implement the smallest fix.
4. Run the focused test again, then a broader relevant test if risk warrants it.

Report the red and green commands in the final answer.

## Style

- Prefer `assert!`/`expect` with clear invariant messages over silent defaults.
- Keep fixtures minimal and focused on the behavior under test.
- Use `cargo run --bin ast -- '<snippet>'` to verify Prism node names/accessors instead of guessing.
