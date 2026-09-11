---
name: performance
description: "Investigate a concrete Ruby Fast LSP indexing, query-latency, or memory problem with comparable before/after measurements."
---

# Performance investigation

Use `docs/development/performance.md` as the single workflow and budget guide.
Do not start a broad optimization campaign for a refactor or prose change with
no observed performance impact.

1. Name the symptom and acceptance question. Fix the workload, runtime, compiler
   profile, resource settings, and cold/warm cache state before collecting samples.
2. Build the release `profiler` and establish a baseline. Use its phase/query
   controls; add CPU or allocation profiling only to resolve a concrete unknown.
3. Change one measured cause. Preserve exact semantic results and all source,
   ownership, cache-key, and resource-governor contracts.
4. Alternate equivalent baseline/candidate runs. Compare raw samples, medians,
   affected p95, memory, and semantic manifests under unchanged ceilings.
5. Accept or revert, record the decision, and stop once the evidence resolves
   the question. Keep local logs under `target/performance/` and a concise durable
   decision record under `support/performance/` only when a maintained contract
   needs it. Routine run reports belong in build/release artifacts.

Read relevant accepted/rejected historical reports before repeating a design.
Old private-corpus timings are context, not a mandate to inspect a private
workspace or certification of a different build. Report unavailable evidence
instead of replacing its workload or relaxing its limits.
