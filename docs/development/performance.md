# Performance workflow

Measure a concrete symptom: slow cold indexing, an edit/query delay, or retained
memory. Use one hypothesis and one comparable before/after experiment. Do not
start a performance campaign for documentation, a file move, or presentation-only
changes unless there is evidence that they affect a hot path.

## Everyday loop

1. **Reproduce and establish a baseline.** Keep the same corpus, runtime,
   configuration, machine, build profile, and cache state. Distinguish cold disk
   caches, warm disk caches, and reuse within one running server.
2. **Locate the cost.** Run the profiler below. Use CPU sampling or allocation
   profiling only when the ordinary report does not explain the symptom.
3. **Change one cause.** Preserve semantic output, ownership, cache identity, and
   CPU/task/memory/I/O admission. Run the relevant correctness tests.
4. **Compare and stop.** Alternate baseline and candidate runs, retain raw
   samples, compare medians and affected p95 values, and check fixed budgets.
   Accept a demonstrated improvement; revert an unsupported or regressing
   optimization. Do not broaden the experiment after the evidence settles it.

Build once per revision, then run the same executable repeatedly:

```sh
cargo build --locked --release --bin profiler
mkdir -p target/performance
./target/release/profiler --benchmark-iterations 100 --check-budgets \
  > target/performance/budgets.log 2>&1
```

Without a workspace argument, this runs the deterministic built-in project and
checks the production regression ceilings. For a reported workload, select its
workspace explicitly (an absolute path also works):

```sh
./target/release/profiler \
  --workspace src/test/fixtures/inference/queries \
  --benchmark-iterations 100 > target/performance/queries.log 2>&1
```

Use `--help` for focused definition/reference probes, diagnostic and semantic
export manifests, and explicit resource settings. Query probe coordinates are
zero-based LSP positions. Private workspaces may be inspected read-only; publish
only sanitized evidence and generic reproductions.

## Deeper investigation, only when needed

For CPU sampling, build the release profiler and run it under a sampling tool,
for example `samply record ./target/release/profiler /path/to/project` if samply
is installed. Target the phase indicated by the baseline report.

For allocation investigation, build with
`--no-default-features --features memory-profiling` and run the profiler with
`--memory`. DHAT changes the allocator and writes `dhat-heap.json`; use it to
locate allocation cost, not as a directly comparable production timing sample.
Rebuild the ordinary release executable before evaluating the candidate.

The narrower `profile_indexer`, `profile_project_collection`, `profile_file_open`,
`profile_single_file`, and `bench_references` tools remain available for isolated
questions. They are not additional mandatory gates for every change.

## Acceptance

The profiler is the executable source of truth for the fixed built-in budgets:

| Measurement | Ceiling |
| --- | ---: |
| Cold indexing | 2 s |
| Body-only edit p95 | 100 ms |
| Completion / references p95 | 50 ms |
| Hover / definition / semantic diagnostics p95 | 25 ms |
| Estimated engine heap | 32 MiB |

These are regression ceilings for the built-in workload, not latency promises
for every project or machine. Material inference expansions also preserve the
existing comparison contract: median wall/CPU and affected edit/query p95 stay
within 3% outside measured noise. For the previously accepted fully warm,
two-project workload, preserve its 1,776,846,438-byte peak RSS ceiling. That
corpus-specific ceiling is recorded in the retained
[callable-body acceptance](../../support/performance/callable-body-inference-final-2026-08-12.json);
it is not a universal memory target or an extra fixture
required for prose changes. If the workload is unavailable, report the gap;
never substitute a different workload and claim equivalent evidence.

Keep semantic fingerprints and exact expected results alongside timings. Never
raise a ceiling, weaken a proof, or suppress diagnostics to accept a candidate.
A faster incorrect result is a failed experiment.

A retained report needs only: problem/hypothesis, baseline and candidate identity,
reproducible inputs/commands, cache state, raw samples, semantic checks, and an
accepted/rejected decision. Put local logs in `target/performance/` and durable
reports with build/release artifacts. Promote a report to
[support/performance/](../../support/performance/README.md) only when it explains a
still-active contract, and link it from that contract. Remove superseded run
reports from the working tree; Git history preserves committed evidence. Retained
decision records keep their original measurements and paths and do not certify
a new build.

For a release candidate, run the existing
[simulation release gate](release.md), which already includes the deterministic
profiler. Do not add a second copy of the same gate to a release procedure.
