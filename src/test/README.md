# Testing Ruby Fast LSP

Choose the smallest harness that exercises the behavior's real owner. Most
feature tests are internal Rust tests in `integration/`; Cargo runs them with
`cargo test --locked --lib`. Shared fixtures live in `fixtures/`.

| Need | Use |
| --- | --- |
| One indexed Ruby snippet | `harness::check()` with inline tags |
| Static cross-file behavior | `harness::check_multi_file()` |
| Open/edit/save/close, reindexing, or delayed work | `harness::FakeEditor` |
| Pure graph, inference, parser, or contract behavior | A focused unit test beside the owning code |
| Public LSP initialization / extension integration | [crates/lsp-test-harness](../../crates/lsp-test-harness/) |
| Real CLI exit code and serialized output | [cli/process.rs](cli/process.rs) |
| Generated semantic/edit scenarios and controlled schedules | [simulation guide](../../docs/development/simulation.md) |

## CLI test organization

`cli/mod.rs` contains internal `CheckSession` and CLI/LSP parity tests.
`cli/process.rs` launches the real `ruby-fast-lsp` executable and checks its exit
code, stdout/stderr, and JSON report. Cargo.toml registers it as the separate
`check_cli` integration-test target, supplying `CARGO_BIN_EXE_ruby-fast-lsp` even
though its source lives here. It is not compiled as a module of the library test
suite. Both targets run in `cargo test --workspace`; no root `tests/` folder is
needed.

## Add a regression

Reduce a reported defect to neutral Ruby names and minimal source. First write
and run an integration test that fails on the intended semantic assertion. Fix
the owning production layer, then run that test and related coverage. Add a
recovery observation when an edit or delayed result caused the defect. Do not
copy application code, private paths, or business examples into the repository.

Use `harness::fixture_uri` and `fixture_path` for in-memory files. They preserve
the same fixture names while producing absolute paths on every supported OS.
Use temporary directories for disk-backed cases. Avoid hardcoded Unix file URLs
and compare filesystem paths as paths, including when checking suffixes.

Inline fixtures use `$0` for the cursor and tags such as `<def>`, `<ref>`,
`<type>`, `<err>`, `<warn>`, `<hint label="...">`, and
`<complete items="..." excludes="...">`. The
[harness module](harness/mod.rs), [tag parser](harness/fixture.rs), and
[assertion runner](harness/check.rs) define the actual syntax. Prefer an existing
nearby feature test over inventing a new fixture convention.

```sh
cargo test --locked --lib test_name
cargo test --locked --workspace
cargo test --locked --test check_cli
```

The workspace suite includes ordinary simulator cases with no ignored tests.
Explicit scale and read-only corpus campaigns use the opt-in `simulation` binary;
see the [commands and boundaries](../../docs/development/simulation.md#explicit-campaigns).
The independent validation workflow runs both synthetic campaigns and records an unselected corpus
as not run. Do not add an ignore to conceal a failing or flaky regression.

## What FakeEditor observes

FakeEditor initializes an ordinary `LspService` and consumes its outbound
`ClientSocket`. Async `diagnostics()` waits for the submitted value to arrive as
`textDocument/publishDiagnostics`; absent delivery differs from an explicit
empty clear. The reader acknowledges protocol requests without performing analysis.

`published_diagnostics()` and inline diagnostic tags observe synchronous
submission, useful while a deterministic race gate holds production identity
locks. Lifecycle/query helpers invoke production handlers directly, so this is
not full inbound LSP transport or an installed-editor test. The external harness
and package smoke tests provide separate evidence. Do not merge the external
harness into root-crate internals: it already depends on the server crate.

Assertion helpers must be read-only. They must not reindex, resolve, refresh, or
repair the state they are about to check. Require complete target sets/ranges
where identity matters, including missing, extra, and duplicate output controls.

## Deterministic process and race tests

Use [with_process_clock](harness/process.rs) for subprocess argv, stdin, resource
admission, and deadline contracts. Keep real child processes and production
functions. For timeouts, wait for child readiness before advancing the controlled
clock. An independent wall-clock watchdog guards hangs; it is not the asserted
product deadline. Do not solve scheduler flakiness by lengthening production timeouts.

Real decompiler acceptance tests hold `isolate_decompiler_budget()` for their
lifetime. Independent tests share the process-wide two-child limit and must not
consume each other's slots. The guard isolates tests; production admission,
memory limits, timeouts, and navigation assertions remain active. Permit exhaustion
and recovery have their own explicit regression.

Use existing schedule gates to pause collection/commit/publication and perform
normal editor operations across that boundary. A test-only observer or gate is
valid when it exercises the production flow; a test-only alternative semantic
implementation is not.

## Acceptance contracts

[scorecard.toml](../../support/type_inference/scorecard.toml) and
[real_project_precision.toml](../../support/type_inference/real_project_precision.toml)
are reviewed inference expectations. Their report tests run in the ordinary
workspace suite and explicitly in the validation workflow. Fixture size, code coverage,
and passing counts do not measure how many future user defects the simulator
will detect. Keep unsupported or unexercised forms visible.
