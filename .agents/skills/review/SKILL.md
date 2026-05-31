---
name: review
description: "Review Ruby Fast LSP changes for correctness, architecture boundaries, tests, and TigerBeetle-style invariants."
---

# Review

Use this skill for code review, PR review, or pre-merge checks. Findings come first, ordered by severity, with file/line references.

## Blocking Checks

- The change preserves the `ruby-analysis` vs `src/` boundary from `AGENTS.md`.
- Method lookup, MRO, unresolved-method diagnostics, and ambiguity policy are not duplicated outside engine resolution.
- Reindexing uses `register_file -> replace_facts -> resolve` and does not reintroduce broad inline affected-file fanout on edit.
- No `debug_assert!`; production invariants use `assert!`, `panic!`, or `expect` with actionable messages.
- No silent fallback for invalid internal state.
- No wildcard `_` match arm for an invariant panic/unreachable case when explicit variants are possible.
- Tests cover new behavior or regression risk.

## Architecture Checks

- `src/query/*` should adapt LSP/document context to `AnalysisQuery` and map domain ranges to protocol locations.
- Reusable graph/type/fact logic belongs in `crates/ruby-analysis`.
- `ruby-analysis::engine` should not depend on `ruby-analysis::inference`; inference may ask engine questions through query contracts.
- Public APIs should expose domain views, not arena/store internals.
- Snippets, trigger routing, and completion item shaping stay in `src/capabilities/completion`.

## Test Checks

- Scenario-driven bug fixes should show a failing test before implementation.
- Prefer `check()` for single-pass feature tests and `FakeEditor` for lifecycle behavior.
- Black-box tests belong in `crates/lsp-test-harness` only when they need public LSP startup/package behavior.

## Review Output

Use this structure:

1. Findings, ordered by severity.
2. Open questions or assumptions.
3. Brief change summary only if useful.

If no issues are found, state that clearly and mention remaining test gaps or residual risk.
