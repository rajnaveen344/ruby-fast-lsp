# Simulation testing

The simulator generates Ruby projects and editor operations, runs ordinary
production analysis, and checks observations against explicit expectations.
It tests more than crashes: navigation, selected types/diagnostics, stale state,
project isolation, and recovery. Its coverage is limited to the modeled forms
and observations; a passing run is not a percentage of Ruby feature coverage.

## Architecture

Source lives under [src/test/simulation/](../../src/test/simulation/). The
`support/` module is shared by ordinary regression tests and an opt-in
`simulation` executable; it is absent from normal server builds. The executable
runs explicit acceptance campaigns directly, without invoking Cargo tests or
compiling test-only server fields.

| Part | Responsibility |
| --- | --- |
| `support/project.rs`, `support/graph.rs` | Independent model of namespaces, methods, edges, and edits |
| `support/ruby_gen.rs` | Ruby source plus exact token/source mapping for modeled sites |
| `support/oracle.rs` | Expected behavior from the model, independent of production results |
| `runner.rs`, `support/engine_runner.rs` | Exercise the LSP-handler harness or analysis engine and compare observations |
| `support/seeded.rs`, `support/regression_seeds.txt` | Bounded generated scripts and retained failure seeds |
| `consistency.rs` | Compare incremental results with a fresh analysis |
| `contracts/exact.rs`, `contracts/observations.rs` | Complete response/lifecycle contracts and observer controls |
| `production_schedules.rs`, `dependency_refresh.rs`, `interleavings.rs` | Deterministically pause real work around edits, commits, and publication |
| `support/build_identity/` | Source/build/executable identity retained with replay evidence |

The model and production engine must remain independent. Fresh/incremental
agreement tests consistency, but both engines can agree on the same wrong answer.
Reviewed Ruby oracle cases provide another check of selected dispatch semantics;
a single executed path still does not prove static-analysis completeness.

The generated model covers namespace/method relationships, supported constant
and method navigation, selected references and type observations, and selected
diagnostics. Method navigation checks complete target sets and independent
semantic precedence constraints. Coverage buckets in `tests.rs` require modeled
forms, including implicit module dispatch. Unsupported sites are recorded as
coverage gaps, never silently turned into successful assertions.

Exact method navigation/reference/rename contracts, outbound-diagnostic controls,
and delayed-production schedules complement that model. Many other advertised
features are tested in ordinary integration tests without a generated oracle:
for example completion, signature help, code actions, formatting, and hierarchies.
A new handwritten regression does not automatically teach the generator that feature.

## Add a useful red scenario

1. Create a minimal generic regression with the full expected response: target
   URIs and ranges, edits and replacement text, or diagnostic codes/ranges.
   Include unrelated same-name symbols when testing identity.
2. Exercise ordinary open/edit/save/close handlers with `FakeEditor` for lifecycle
   bugs. Use a production schedule gate for delayed work. Observe output without
   repairing analysis state; distinguish missing publication from an empty clear.
3. Run the test before the fix and require the intended assertion failure.
   A build error, setup panic, or watchdog timeout is not semantic red evidence.
4. Fix the owning production layer. Check the original case, the edit transition,
   and restoration/reopen when relevant.
5. To expand generated coverage, extend the project model, renderer/source map,
   independent oracle, observer, and relevant edit generation. Add a required
   coverage bucket so a seed change cannot silently remove the new form.
6. Add comparator controls for wrong/missing/extra results when introducing an
   observer. Add an isolated production mutation to the fault inventory when
   claiming that the simulator detects that class of defect.

Use `contracts/exact.rs` as a complete-output example and `support/seeded.rs`'s module-dispatch
scenario as an example of expanding generated semantics. The
[test guide](../../src/test/README.md) explains the production-handler versus
transport boundary.

## Run and replay

```sh
cargo test --locked --lib simulation::
SIM_SEED=42 cargo test --locked --lib simulation:: -- --nocapture
SIM_RANDOM_SEEDS=10 cargo test --locked --release --lib simulation::
```

Failing seeded runs retain generated source, script, generator identity, replay
instructions, and source/build metadata. Use the reported artifact directory and
command; compare source and executable identities before interpreting a replay
from a different build. Reduce the example and retain its seed in
`support/regression_seeds.txt`. A seed alone is not an identity-independent reproduction.

Controlled schedules preserve the chosen ordering. Keep production clock,
resource admission, and publication boundaries in the replay; arbitrary sleeps
cannot reproduce a race reliably.

## Explicit campaigns

Scale and real-corpus acceptance are commands, not ignored tests:

```sh
cargo run --locked --release --features simulation --bin simulation -- scale-lsp
cargo run --locked --release --features simulation --bin simulation -- scale-engine
cargo run --locked --release --features simulation --bin simulation -- corpus --root /absolute/path/to/ruby-project
```

`scale-lsp` samples navigation, references, hover, and hints on a generated large
project. `scale-engine` checks all modeled edges through the analysis engine.
`corpus` requires an explicit read-only directory and retains the existing size,
semantic-sample, and time-budget assertions. Missing or unsuitable inputs fail;
they never become a passing test. Each successful command emits one completion
report identifying its campaign. Release validation rejects missing, duplicate,
or mismatched reports and rejects every ignored Rust test.

Generated editor edits open all changed files before sending changes. The oracle
tracks only delivered revisions. Separate lifecycle contracts verify that an
unnotified edit to a closed file stays invisible until reopen, then check removal
and restoration. Retained failing seeds and generated delivery controls keep
that distinction executable.

## Broader gates and limits

The [release runner](release.md) adds release-mode simulations, explicit synthetic
scale tests, reviewed Ruby oracle execution, and fixed performance budgets.
Real-corpus checks require an explicitly selected read-only workspace. Scale,
Ruby semantics, and installed-editor behavior are different evidence.

[support/simulation/](../../support/simulation/) holds the fault campaign and
independent Ruby oracle controls. A mutation counts as detected only when its
intended semantic assertion fails. Compilation errors, unrelated panics, missing
inputs, and timeouts are not successful detection. The campaign retains its
patch, logs, and build identity in an isolated checkout.

Keep improving exact independent expectations for unmodeled features, controlled
restart/transport scenarios, and same-failure replay/reduction. Consult the
current tests and runner output for exercised cases; avoid permanent numerical
ratings, historical test-count tables, or claims of complete feature coverage.
