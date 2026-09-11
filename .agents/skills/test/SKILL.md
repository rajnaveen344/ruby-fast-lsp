---
name: test
description: "Add or debug Ruby Fast LSP regressions, choose check/FakeEditor/process harnesses, and repair flaky tests."
---

# Tests and regressions

Read `src/test/README.md` for harness boundaries and
`docs/development/simulation.md` when changing generated coverage.

1. Reduce a concrete bug to generic Ruby source and a complete expected result.
   Write and run the regression before the production fix; require the intended
   semantic assertion to fail. Setup or compiler failures do not establish red.
2. Use `check()` for one pass, `check_multi_file()` for static cross-file cases,
   and `FakeEditor` for lifecycle transitions. Use the external LSP harness or
   `src/test/cli/process.rs` only when that public/process boundary matters.
3. Follow existing inline tag examples in `src/test/harness/mod.rs` and nearby
   integration tests. Confirm unfamiliar Prism nodes with
   `cargo run --bin ast -- --loc '<neutral Ruby snippet>'`.
4. Keep observations read-only. Compare complete identities/ranges where needed;
   missing publication must not satisfy an empty-output assertion. Add restoration
   or reopen checks for edit-dependent bugs.
5. Use `with_process_clock` and real children for subprocess contracts. Wait for
   child readiness before advancing a timeout clock. Use production schedule gates
   for races, not sleeps or larger deadlines.
6. Run the focused green test, related coverage, and the workspace suite for
   shared semantic/lifecycle changes. Report actual results and any deferrals.

A handwritten test does not add a generated semantic form. Simulator expansion
also needs a model, source mapping, independent oracle, observation, edits where
relevant, and a required coverage bucket. Do not remove a useful contract or add
an ignore merely because its current synchronization is flaky.
