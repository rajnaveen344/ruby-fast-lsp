---
name: ruby-index
description: "Debug the Ruby Index tree view, namespace graph, exported JSON, mixin resolution, included_by, and MRO."
---

# Ruby Index

Use this skill for the VS Code Ruby Index tree, namespace hierarchy, mixins, `included_by`, MRO, or graph export debugging.

## Current Product Goal

`goal.md` treats hierarchical Ruby Projects as **done (M7)**. Indexing
performance is maintenance: defend RSS/fingerprints/readiness, do not open
endless wall-clock micro-opt campaigns.

## Current Ownership

- UI/tree behavior lives in the VS Code extension under `vsix/`.
- LSP request handling and command response shaping live in `src/`.
- Namespace tree, graph, mixin, and hierarchy truth live in `ruby-analysis::engine`.
- Query adapters should use `ruby_analysis::engine::AnalysisQuery`.

## Common Debug Path

1. Reproduce with a tiny Ruby fixture.
2. Use the Ruby Index export/debug command if the issue is visible in the tree.
3. Inspect engine facts through `AnalysisQuery::debug_*` or focused tests.
4. Verify whether the issue is fact collection, engine resolution, adapter shaping, or extension display.

## Things To Check

- Class/module `GraphNodeKind` is explicit and not defaulted.
- Include/prepend/extend edges point in the expected direction.
- Singleton class edges are distinct from instance namespace edges.
- External types filtering is applied only at projection/display time.
- MRO order matches Ruby semantics and engine resolution policy.
- Ambiguous method definitions should resolve references but suppress unresolved-method diagnostics.

## Test Guidance

- Use `check()` for direct navigation/reference behavior.
- Use `FakeEditor` when tree/debug results depend on lifecycle indexing.
- Add engine-level tests when the bug is in graph or hierarchy semantics.
- Add extension tests only when server output is correct and display logic is wrong.
