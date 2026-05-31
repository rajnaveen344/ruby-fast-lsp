---
name: performance
description: "Profile and optimize Ruby Fast LSP latency, indexing, memory, and query hot paths with measurement-first workflow."
---

# Performance

Use this skill for slow indexing, high memory use, editor latency, benchmark design, or performance-sensitive architecture decisions.

## Ground Rules

- Correctness first. Do not optimize by weakening invariants.
- Measure before changing code.
- Keep editor typing latency separate from project-wide background work.
- Do not reintroduce broad inline affected-file fanout during `didChange`; the May 23 2026 experiment regressed real editing by fanning out to 2186 affected files.

## Likely Hot Areas

- Duplicate parse/fact passes.
- Full-file work on every edit.
- Extension hook overhead.
- Source offset and `TextRange` conversions.
- Repeated engine graph resolution.
- Method lookup, MRO, and unresolved-method suggestions.

## Workflow

1. Reproduce with a focused project or fixture.
2. Capture baseline timing/memory.
3. Identify the hot path with profiling, not intuition.
4. Make one scoped change.
5. Re-run the same measurement and relevant tests.

## Useful Commands

```bash
cargo test
cargo build --release
cargo run --release --bin profile_indexer -- <path>
cargo run --release --bin profiler -- <path>
```

Use existing scripts or binaries before adding new tooling. If new benchmarks are needed, keep fixtures deterministic and checked in only when they are small enough to maintain.

## Design Direction

Future edit performance work should prefer semantic export fingerprints plus bounded or visible-file diagnostic refresh. Project-wide refresh should run outside the typing critical path.
