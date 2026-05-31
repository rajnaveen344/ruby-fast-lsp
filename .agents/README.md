# Ruby Fast LSP Skills

Project skills are intentionally limited to current, distinct workflows:

- `architecture`: current `ruby-analysis` layer boundaries and placement decisions.
- `test`: `check()`, `FakeEditor`, and black-box LSP testing guidance.
- `review`: review checklist for correctness, invariants, boundaries, and tests.
- `performance`: profiling and latency/memory workflow.
- `ruby-index`: namespace tree, mixin, MRO, and graph debugging.
- `release`: version bump, tag, and publish flow.

Removed skills:

- `c4-diagrams`: the LikeC4 diagrams were stale and removed.
- `refactor`: stale module targets and duplicated architecture guidance.
- `error-handling`: folded into `AGENTS.md` and `review`.
- `tigerstyle`: folded into `AGENTS.md` and `review`.

When adding a new skill, keep it narrow and operational. Do not create a skill for guidance already covered by `AGENTS.md`.
