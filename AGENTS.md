# Ruby Fast LSP contributor guide

Ruby Fast LSP is a Rust language server using Prism, tower-lsp, and Tokio.
This guide applies to human contributors and coding tools alike. Keep it about
current ownership and durable rules; put plans in the README roadmap and
measurement results in `support/`. Do not add session logs or historical test
counts to this guide.

## Start here

| Task                                  | Read                                                                                                                                       |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Understand the product and priorities | [README](README.md)                                                                                                                        |
| Find user and developer documentation | [Documentation index](docs/README.md)                                                                                                      |
| Place code or change state ownership  | [Architecture](src/ARCHITECTURE.md), [analysis library](crates/ruby-analysis/README.md), [server owners](docs/development/server-state.md) |
| Add or debug a test                   | [Test guide](src/test/README.md), [simulation guide](docs/development/simulation.md)                                                       |
| Change inference                      | [Inference proof model](crates/ruby-analysis/src/inference/mod.rs), [feature contracts](docs/README.md#feature-contracts)                  |
| Measure or release                    | [Performance workflow](docs/development/performance.md), [release checklist](docs/development/release.md)                                  |

Optional focused workflows live in [.agents/skills](.agents/README.md). Source
manifests, advertised capabilities, handlers, and active tests take precedence
over old status reports. Update the nearest guide when its contract changes.

## Ownership

| Location                            | Owns                                                                                          |
| ----------------------------------- | --------------------------------------------------------------------------------------------- |
| `src/`                              | LSP transport, editor projections, documents, runtime/workspace lifecycle, resource admission |
| `ruby-analysis::core`               | Domain names, ranges, types, facts, source identities                                         |
| `ruby-analysis::indexer`            | Parsing, AST traversal, source mapping, file-fact production                                  |
| `ruby-analysis::engine`             | Semantic state, file replacement, graph/MRO resolution, query and diagnostic policy           |
| `ruby-analysis::inference`          | Type derivation, local flow, signature substitution, bounded equation solving                 |
| `crates/extension-*`, `extensions/` | Extension contracts, hosts, and framework-specific fact producers                             |
| `editors/`                          | Editor UX, distribution packaging, installed-artifact validation                              |

Keep reusable analysis independent of LSP types. `src/query/` adapts cursor and
document context to `AnalysisQuery`/`TypeQuery` and converts domain ranges to
protocol responses. Do not duplicate MRO, identity, ranking, or missing-method
policy in feature adapters. Engine resolution may coordinate inference solvers;
inference may consult engine queries. Engine ownership of solved state remains
singular. Expose domain operations and views, never mutable stores or arena IDs.

## Correctness contracts

- Use production `assert!`/`expect`/`panic!` for broken internal invariants, never
  `debug_assert!`. Messages must identify the broken invariant, why it is a bug,
  and how to fix it. Enumerate impossible variants instead of hiding them in a
  wildcard panic arm. Represent invalid states in the type system where possible.
- Missing evidence in user code is an expected analysis outcome: retain explicit
  `Unknown` reasons. Malformed source, unsupported Ruby, unavailable runtimes, and
  external tool failures must not be treated as corrupt internal state. Never
  replace uncertainty with a guessed concrete type or silently hide a failure.
- Use recursive Prism traversal. Verify unfamiliar nodes with
  `cargo run --bin ast -- --loc '<neutral Ruby snippet>'`. Prism ranges use bytes;
  LSP positions are zero-based UTF-16. Preserve original source coordinates,
  including ERB projections and non-BMP characters.
- Register and replace file-owned facts through the engine lifecycle; resolve
  immediately for interactive work or after a deferred batch. File replacement
  removes old facts. Background producers capture an opaque source snapshot and
  commit only while the same engine/file/revision identity is current. Open
  buffers remain authoritative over disk. Do not add an unconditional late write.
- Every Ruby project owns an isolated engine, runtime, dependencies, and extension
  facts. Route documents by the deepest project root and retain origin for
  external navigation. Share only bounded immutable products, never project
  semantic state. Source ownership comes from `ProjectFilePolicy` and explicit
  `SourceKind`, not scattered path heuristics.
- Keep dependency/runtime selection exact to the owning project. Do not fall back
  to an unrelated global gem, nearby runtime, or inherited shell environment.
  Bundled core semantics remain available when a runtime is missing. Runtime and
  decompiler processes retain their resource limits and error isolation.
- Diagnostics observe current facts. Cold indexing retains closed-file facts but
  publishes only open documents. Unknown lookup edges suppress unsupported
  missing-method claims. Observation helpers must not solve or repair state.
- Types, hover, hints, completion, diagnostics, and CLI checks consume the same
  engine outcome. Canonical `RubyType::Literal`/`Shape` values retain correlated
  alternatives; local Hash identities do not outlive file replacement. Preserve
  documented proof bounds and Unknown reasons on escape, mutation, or overflow.
  A value constant is not automatically a class. Bind constants to current
  equations without unioning stale seed types into the result.
- Navigation first selects token identity, then applies engine-owned semantic
  ranking. Local bindings do not depend on their inferred type. Bracket calls
  anchor the method reference at `[` without swallowing the argument token.
  Keep compact inlay labels separate from full type/shape tooltips; presentation
  must not change inference. Binding hovers show name, type, and binding kind.
- Ruby/RBS/runtime/extension facts use ordinary file-owned write paths. Extensions
  do not own a parallel semantic database. Preserve trust, manifest validation,
  origin, and packaged runtime assets. Rename/edit operations remain fail-closed
  on ambiguous identities, external targets, or unsupported transformations.
- Keep expensive work inside the server's CPU/task/memory/I/O governor and bounded
  caches. Cache identity includes all semantic producer inputs and exact runtime,
  dependency, and source identity; preserve `build.rs` fingerprint coverage.
  Never weaken budgets or bypass admission to make a measurement pass.

## Readable structure

Name modules after their responsibility. Keep state private to its owner and
expose coherent operations. Do not combine unrelated code merely to lower a
file count, or add indirection that only hides fields.

New and reorganized source folders have at most **10 immediate entries**,
including subfolders, `mod.rs`, and README files. The analysis crate is entirely
strict. Other oversized source folders have exact legacy baselines that may
shrink but must not grow. A cohesive node family without a useful semantic split
requires a documented, bounded exception in the structure policy. Vendored
snapshots have explicit ownership exclusions; maintained support folders follow
the ordinary limit.

Run `python3 -B support/structure/check.py`; see the
[policy guide](support/structure/README.md) before changing an exception.
`docs/` holds maintained prose; `support/` holds assets, contracts, validation
scripts, and historical measurements. Generated logs and profiles belong under
`target/`, not in tracked documentation or agent configuration.

## Change and validation workflow

1. For a reported behavior bug, reduce it to a small generic integration test
   and demonstrate the expected assertion failure before fixing production code.
   Private workspaces may be inspected read-only, but never copy their names,
   paths, business terminology, or code into fixtures or documentation.
2. Fix the owning layer. Use `check()` for static feature cases, `FakeEditor` for
   edit/reindex behavior, and the external harness or CLI test for process-level
   contracts. See the test guide for actual boundaries and simulation authoring.
3. Run the focused test, then relevant broader checks. Do not ignore a failing
   test, replace a semantic assertion with a weak subset check, or extend a
   wall-clock timeout to hide flaky scheduling. Subprocess tests use the harness
   process clock and real children when testing argv, stdin, admission, or timeouts.
4. Run formatting and the structure check for Rust/layout changes. Run the full
   workspace suite for shared semantic or lifecycle changes. Use performance
   measurements for changes to hot paths, inference bounds, scheduling, or caches;
   documentation and presentation-only edits do not require a performance campaign.
5. Report what changed, the checks actually run, and any remaining limitation.
   A regression test does not automatically expand generated simulator coverage.
   Keep standard, simulation, native package, and real-editor evidence distinct.

Common commands (run from the repository root):

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
python3 -B support/structure/check.py
node editors/scripts/release_checks.js correctness
node editors/scripts/release_checks.js simulation
./editors/vscode/create_vsix.sh --current-platform-only
```

Release commands run broader checks and retain logs under
`target/release-evidence/`. Committing, installing, and publishing are separate
operations; follow the requested scope and existing authorization. Keep commit
messages generic and preserve unrelated working-tree changes.
