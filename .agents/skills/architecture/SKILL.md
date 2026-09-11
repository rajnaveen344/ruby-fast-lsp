---
name: architecture
description: "Plan Ruby Fast LSP module, API, and state-ownership changes across the LSP adapter and ruby-analysis layers."
---

# Architecture changes

Read `AGENTS.md` and the local guide for the affected owner before choosing a
new abstraction. Start at `src/ARCHITECTURE.md`, `crates/ruby-analysis/README.md`,
and `docs/development/server-state.md`; follow their source links as needed.

1. Identify the state owner and its lifetime: buffer, file facts, isolated project
   engine, process-wide immutable product, or editor projection.
2. Keep parser/fact production in `indexer`, type derivation in `inference`, and
   graph/query/diagnostic policy in `engine`. The LSP adapter converts context and
   responses; it must not introduce a second semantic resolution policy.
3. Read through `AnalysisQuery`/`TypeQuery`; write through the existing file-fact
   lifecycle. Expose domain operations, not mutable stores. Engine and inference
   can cooperate inside the analysis crate while preserving engine state ownership.
4. Preserve source snapshots, project isolation, lock lifetimes, resource
   admission, and cache identity when moving state. Avoid global lock consolidation
   or accessors that merely make every internal field public again.
5. Group files by semantic responsibility. Follow the ten-entry policy in
   `support/structure/README.md`; do not grow legacy baselines or merge unrelated
   code solely to satisfy the count.
6. Update affected reading guides and run structure/format checks plus tests that
   exercise the changed boundary. Structural moves alone need no performance
   campaign; behavior or hot-path changes may need focused measurements.

For collector changes, consult
`crates/ruby-analysis/src/indexer/fact_collector/README.md`; for flow ownership,
consult `crates/ruby-analysis/src/inference/type_tracker/README.md`.
