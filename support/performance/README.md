# Retained performance decisions

These five records explain contracts still used by the implementation or feature
guides. They retain their original measurements; none certifies the current build.

| Record | Why it remains |
| --- | --- |
| [Hash shape bounds](type-inference-shape-bounds-2026-08-11.json) | Measurement and rationale for the fixed fields, depth, variants, aliases, and solve limits |
| [Hash shape acceptance](type-inference-shapes-final-2026-08-12.json) | Accepted structural inference behavior under latency and memory budgets |
| [Higher-order call acceptance](higher-order-call-inference-final-2026-08-12.json) | Accepted block/callable signature bounds and performance contract |
| [Callable-body acceptance](callable-body-inference-final-2026-08-12.json) | Accepted callable proof bounds and the fixed warm-workload RSS ceiling |
| [Resolve instrumentation](resolve-pass-cache-cardinality-2026-08-01.json) | Why production diagnostics keep combined timing while detailed cardinality was measured separately |

Use the [performance workflow](../../docs/development/performance.md) for new
experiments. Write routine logs and reports under `target/performance/` and attach
release evidence to the candidate. Retain a record here only when a maintained
contract references its decision; remove it when that contract is superseded.
The ordinary source-folder limit applies here too.

Old intermediate profiles, one-off regression runs, and superseded acceptance
checkpoints are available in Git history under `support/performance/`. They are
not active test inputs and do not need an archive in every checkout.
