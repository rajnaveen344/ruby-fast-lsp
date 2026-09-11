---
name: review
description: "Review Ruby Fast LSP changes for observable correctness, ownership, regression coverage, and readable APIs."
---

# Review

Read the diff and the affected owner, then apply `AGENTS.md`. Follow local source
guides instead of relying on past task summaries or test counts.

Prioritize concrete defects:

- Wrong symbol/type/range, invented proof, stale facts after edits, or unsupported
  edits to ambiguous/external declarations.
- Project isolation, source-snapshot, cache-identity, lock, or resource-admission
  regressions.
- Duplicated method/MRO/diagnostic policy outside the engine; mutable store leaks
  or state with unclear ownership.
- Observers that repair state, subset assertions that hide extra targets, ignored
  failures, scheduling-sensitive deadlines, or missing recovery coverage.
- Misleading public behavior, stale source links, unclear module responsibilities,
  or new source folders that violate the structure policy.

Check that evidence matches the claim: ordinary tests, generated semantics,
transport, installed packages, and measured performance establish different
things. Use `src/test/README.md` and `docs/development/performance.md` to choose
additional checks only when an unresolved concern warrants them.

Report actionable findings by severity, with file/line references, a trigger,
and the resulting user impact. Separate open questions from established defects.
If no issues are found, say so and state material untested boundaries without
inventing findings or claiming exhaustive correctness.
