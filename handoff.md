# Handoff: 9/10 Ruby Type Inference and Standalone Type Checking

## Repository and state

The work belongs in:

```text
/Users/naveenraj/sources/devtools/ruby-fast-lsp
```

The repository is on `main`, and the working tree contains substantial in-flight
type-inference, scorecard, CLI, test, performance-evidence, and `goal.md`
changes. Treat those changes as intentional work owned by the user. **Do not
reset, check out, overwrite, or discard them.** Inspect the current diff before
editing overlapping files.

Read [AGENTS.md](AGENTS.md) before making changes. It is the canonical project
guide and contains mandatory correctness, architecture, testing, performance,
and fail-fast rules. Read [goal.md](goal.md) in full before selecting work.

## What we are trying to achieve

The active product goal is to make Ruby Fast LSP's type inference accurate,
conservative, reusable, and measurable:

> Build a modular, editor-agnostic, proof-first Ruby type inference engine that
> infers and uses precise types throughout the semantic system, reaches at least
> 9/10 on a checked-in conformance scorecard, exposes the same analysis through
> LSP features and a standalone `ruby-fast-lsp check` CLI, and does so without
> regressing interactive latency, indexing/checking throughput, determinism, or
> memory.

“Infer and use type” is important. A type is not finished merely because it was
computed or stored. The same solved type must drive hover, inlay hints,
completion, method resolution, definition/references, signature help,
diagnostics, chained calls, dependent-file invalidation, and CLI output. No
consumer should maintain a competing inference algorithm.

The design is proof-first:

- Publish a concrete type only when all required static evidence is present,
  current, complete, and unambiguous.
- Use an explicit, machine-readable `Unknown` reason when proof is incomplete.
- Never turn missing evidence into `Object`, a partial union, an arbitrary
  overload, a confidence-based guess, or a type learned from a convenient call
  site.
- Preserve deterministic results across file order, worker schedules, cache
  state, repeated runs, and CLI versus LSP execution.
- Treat RBS, YARD, bundled runtime signatures, JRuby facts, and validated
  extension facts as evidence flowing through the same engine-owned precedence
  and lifecycle rules.

## What “9/10” means

The phrase is a release criterion, not a subjective impression. The checked-in
scorecard in `support/type_inference/scorecard.toml` defines a 100-point corpus
covering local flow, calls, methods and blocks, RBS and generics, cross-file
inference, generated/runtime facts, lifecycle behavior, explanations, and
CLI/LSP parity.

The goal is eligible for completion only when all gates in `goal.md` pass,
including:

- at least **90/100 overall** and at least **85% in every category**;
- zero known wrong concrete types in supported scorecard cases;
- zero known false-positive type diagnostics in the reviewed simulation and
  real-project corpora;
- supported-site `Unknown` results below the stated limit, each with a stable
  machine-readable reason;
- normalized CLI and LSP types and diagnostics are byte-for-byte equivalent on
  parity fixtures;
- all semantic consumers use the shared solved results;
- `ruby-analysis` is free of `tower-lsp` dependencies and the checker runs
  without starting an LSP service;
- repeated runs preserve types, diagnostics, and semantic fingerprints; and
- every latency, CPU, indexing/checking, incremental, cache, and memory gate
  passes.

A high score from a small or biased corpus is not enough. Safety cases that
correctly return `Unknown` are important regressions, but they do not inflate
accuracy. Fixtures must not be added, removed, or reweighted merely to improve
the number.

## Performance is part of correctness

“Without regressing performance” must be demonstrated with recorded,
like-for-like release-build evidence rather than assumed from focused unit
tests.

The active hard constraints include:

- active-buffer parsing and same-file navigation remain within **500 ms p95**;
- body-only edit, project check/index, and ready-project query latency/CPU stay
  within the M0 baseline noise envelope and never regress beyond the goal's
  **3%** cap;
- body-only edits do not synchronously fan out through closed files;
- the ordinary source pass keeps one Prism parse and one primary scope-aware
  traversal per file;
- unions, recursive types, solver iterations, diagnostics, explanations, and
  caches remain explicitly bounded; and
- warm two-project `goshposh` peak RSS remains at or below
  **1,776,846,438 bytes**.

Use alternating baseline/candidate runs on the same machine, inputs, Ruby/JRuby
runtime, lockfiles, cache state, governor settings, and build identity. Preserve
the raw evidence and accepted/rejected decision under `support/performance/`.
Existing rejected optimization shapes documented in `AGENTS.md` remain
rejected unless new profiling supports a materially different design.

## Architectural direction

The target is one reusable semantic pipeline with thin adapters:

```text
Ruby/RBS/runtime/extension inputs
              |
              v
one offset-preserving Prism parse and scope traversal
              |
              v
file-owned facts, bindings, constraints, and provenance
              |
              v
engine graph plus bounded deterministic inference solve
              |
              v
solved types and domain diagnostics
        +-----+-----+
        |           |
        v           v
   LSP adapter   check CLI adapter
```

Respect the ownership boundaries already defined in `AGENTS.md` and
`goal.md`:

- `ruby-analysis::core` owns domain types, facts, ranges, provenance, proof
  outcomes, and diagnostic contracts.
- `ruby-analysis::indexer` owns Prism parsing and scope-aware fact/binding
  collection.
- `ruby-analysis::inference` owns type rules, flow, narrowing, joins, calls,
  generics, and bounded solving.
- `ruby-analysis::engine` owns file/workspace semantic state, graph truth,
  invalidation, deterministic resolution, and stored solved results.
- The reusable check session owns editor-independent project loading and check
  orchestration.
- The LSP and `check` adapters own only transport, arguments, exit codes, and
  presentation.

There must be one method/MRO/visibility/ambiguity policy, one file replacement
lifecycle, one signature/source-precedence policy, and one semantic result for
all consumers. Do not create a second project model or type checker for the
CLI, put type rules in handlers, expose raw engine stores, or add another AST
walk for each feature.

## Standalone CLI outcome

The supported headless surface is:

```text
ruby-fast-lsp check [PATH ...]
```

It should check the current project, explicit files, directories, or isolated
projects discovered in an umbrella workspace using the same source ownership,
runtime, Bundler, gem, stdlib, RBS, extension, and engine lifecycle as the LSP.
It must provide deterministic human-readable and versioned JSON output, stable
domain codes and ranges, documented exit codes, summary/evidence counters, and
no source mutation.

Automatic annotation insertion or stub generation is outside the initial
checker goal. Those may later consume the same solved types as separate tools.

## How Pyrefly should be used

Pyrefly is an architectural reference, not a behavior oracle and not code to
port. The useful ideas are:

- separate module exports and binding/fact collection from type solving;
- represent flow joins and recursive dependencies explicitly;
- use bounded deterministic solving rather than traversal-order guesses;
- share one analysis engine between CLI and editor integrations; and
- use file/module-level incrementality until profiling justifies a finer model.

Ruby semantics remain authoritative. The implementation must account for
reopened classes, singleton classes, include/prepend/extend, blocks and yield,
visibility, refinements where supported, RBS, Rails/framework-generated facts,
and dynamic Ruby boundaries. Do not import Python module, narrowing, container,
or annotation behavior when it is not valid for Ruby.

## Current checkpoint

`goal.md` already contains the active goal, architecture, scorecard, milestones,
performance gates, definition of done, and a dated implementation checkpoint.
The worktree also contains an in-progress `check` command, proof-carrying type
outcomes and Unknown reasons, recursive method-return solving, new scorecard and
CLI/LSP parity tests, and focused performance artifacts.

According to the current checkpoint:

- the reviewed seed scorecard reports **100/100 on 68 cases**;
- `score_eligible` remains **false**;
- the seed result must **not** be presented as achieving the product-level
  9/10 goal;
- normalized parity, Unknown explanations, diagnostic precision and
  real-project review still need broader coverage; and
- the complete alternating base-versus-candidate cold/warm/edit/query,
  allocation, worker-schedule, and fixed `goshposh` RSS matrix remains a
  completion requirement.

Before continuing, verify the current code and test state; do not assume every
checkpoint bullet is green merely because it is written down. Preserve or
correct the checkpoint based on reproducible evidence.

## How `goal.md` should be used

Use `goal.md` as the product and acceptance contract for a long-running series
of implementation slices:

1. **Orient:** Read `AGENTS.md`, `goal.md`, the relevant architecture docs, the
   current diff, and the evidence associated with the intended slice.
2. **Choose a bounded slice:** Tie it to one milestone, one scorecard gap,
   one proof failure, or one measured performance bottleneck. Do not attempt a
   wholesale rewrite of the inference engine.
3. **Establish red:** Add or select the smallest scorecard/regression,
   CLI-parity, lifecycle, or benchmark case that demonstrates the missing
   behavior, then prove it fails for the intended reason.
4. **Implement in the owning layer:** Extend shared domain inference or engine
   behavior first. Keep LSP and CLI changes as projections over that result.
5. **Prove safety and consumers:** Test the concrete result, Unknown boundary,
   stale-fact removal, deterministic order, and every affected consumer. A new
   concrete inference path needs a wrong-guess/partial-evidence counterexample.
6. **Measure proportionally:** Run focused tests while iterating, then the
   required release comparison for any material work or retained state. Reject
   a slice that violates a hard gate even if accuracy improves.
7. **Record evidence:** Update the machine-readable score or performance
   artifact and the existing implementation-checkpoint section with the exact
   result, limits, and remaining gap. Record rejected approaches as well as
   accepted ones.
8. **Claim only what is proven:** A milestone or isolated score increase is not
   the whole goal. Mark completion only after the complete definition of 9/10,
   packaging, real-project, determinism, and performance matrix passes.

The top “Reusable Goal Text” is the concise brief to give a new agent or
collaborator. The rest of `goal.md` constrains interpretation: it defines proof
safety, architecture, scoring, performance, milestones, tests, non-goals, and
the final completion gate. Do not copy only the score target while omitting
those constraints.

`goal.md` is a living contract, but not a scratchpad or chat log. Update the
current checkpoint in place with concise, dated, evidence-backed facts. Keep
the scorecard and performance artifacts machine-readable and authoritative;
link to them instead of pasting large raw logs into the goal.

If `goal.md` and the implementation diverge, first determine which is stale.
`AGENTS.md` remains authoritative for mandatory repository rules. Reconcile the
documents explicitly rather than silently violating one of them.

## Recommended next work

Continue M0 from the current checkpoint before claiming broader milestones:

1. Run the focused scorecard, checker, parity, inference, and lifecycle tests
   already present in the worktree and fix any failures without discarding
   unrelated changes.
2. Extend normalized CLI/LSP parity to remaining signature and diagnostic
   projections.
3. Add stable Unknown explanations for remaining local-flow expressions.
4. Broaden reviewed diagnostic-precision and reduced real-project fixtures.
5. Complete alternating release-build baseline/candidate measurements for cold
   and warm checking/indexing, body-only and exported-type edits, ready-project
   queries, allocations/CPU, scheduling determinism, and the hard two-project
   `goshposh` RSS ceiling.
6. Update `score_eligible` only when its documented eligibility conditions are
   genuinely satisfied.

After M0 is evidence-complete, advance through the milestones in `goal.md`
rather than assuming their numerical order means the current implementation
has none of their foundations. Preserve working features and migrate toward the
target architecture incrementally.
