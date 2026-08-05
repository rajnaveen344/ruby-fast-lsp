# Ruby Fast LSP: 9/10 Type Inference and Standalone Type Checking

## Reusable Goal Text

Build a modular, editor-agnostic, **proof-first and deterministic** type
inference engine for Ruby Fast LSP. It must infer precise, explainable Ruby
types across local flow, methods, chained calls, blocks, files, RBS declarations,
and supported framework-generated APIs, then use the same semantic result for
hover, inlay hints, completion, method resolution, navigation, signature help,
diagnostics, and a standalone **`ruby-fast-lsp check`** CLI.

Reach a measured **9/10 for type inference**: at least 90/100 on a checked-in
Ruby conformance scorecard, with no critical category below 85%, no silent
wrong-type guesses, and exact CLI/LSP semantic parity. A concrete type may be
published only when complete, current static evidence proves it. If any
required premise, lookup edge, branch, overload, dependency, or solver step is
missing, stale, unsupported, or ambiguous, the result must be an explicit,
machine-explainable **Unknown**. Unknown is safer than a plausible guess and
does not earn precision credit; improve the score by proving more cases, never
by weakening the proof rule or widening an unproven value to `Object`.

For identical source, signatures, configuration, and dependency state, results
must be identical regardless of indexing order, worker scheduling, cache state,
or whether analysis runs through the CLI or LSP. Follow Pyrefly's useful
architectural principles—separate fact/binding collection from solving, model
flow and recursion explicitly, and share one engine between CLI and editor—but
adapt them to Ruby rather than porting Python semantics.

Achieve this without regressing performance. Keep inference incremental and
bounded, preserve existing interactive readiness and memory budgets, and
require recorded before/after evidence for every material inference expansion.

## Product Outcome

A Ruby developer should be able to:

- Open an unannotated or partially annotated project and receive useful,
  conservative types without first adopting a full RBS codebase.
- See the same inferred type in hover, inlay hints, completion, navigation,
  signature help, diagnostics, and CLI output.
- Follow types through assignments, branches, guards, loops, blocks, method
  calls, constructors, inheritance, mixins, constants, and multiple files.
- Use RBS, YARD, bundled core signatures, JRuby signatures, and extension facts
  as typed inputs without creating a second lookup or precedence policy.
- Run ruby-fast-lsp check locally or in CI without starting an LSP client.
- Trust that a reported type error is backed by a complete lookup and an
  explainable evidence chain, not a guess caused by missing code.
- Edit a ready project without project-wide type checking entering the typing
  critical path.

## Meaning of “Infer and Use Type”

Inference is not complete when a type is merely stored. A type must affect the
appropriate semantic consumers:

| Inferred knowledge | Required consumers |
| --- | --- |
| Expression and variable type | hover, inlay hints, completion receiver |
| Call receiver and selected overload | definition, references, completion, diagnostics |
| Parameter and block contract | signature help, argument diagnostics, block inference |
| Method return type | chained calls, assignments, hover, cross-file callers |
| Narrowed flow type | branch-local hover, completion, diagnostics |
| Constant or attribute value type | navigation, completion, downstream calls |
| Public inferred signature | dependent-file invalidation and CLI checking |

One consumer must not independently re-infer a conflicting type. If a feature
needs a different projection, it maps the shared domain result rather than
creating another semantic algorithm.

## Proof-First Determinism

Determinism is necessary but not sufficient. It means byte-identical inputs and
configuration always produce the same types and diagnostics regardless of file
order, hash iteration, worker scheduling, cache state, or CLI/LSP surface.

The stronger rule is **proof-first inference**:

- A concrete type may be published only when every premise in its derivation is
  present, current, and statically defensible.
- Method and chained-call types require a complete receiver lookup chain and one
  unique method/overload result. If one link is Unknown or ambiguous, that
  call's result and every dependent chain segment remain Unknown.
- A union is concrete only when its members exhaust every reachable proven
  outcome. An incomplete branch does not become a partial union; it makes the
  result Unknown.
- Narrowing is concrete only while the guard, reachability, mutation, and alias
  assumptions remain proven.
- A structured partial result such as Array[Unknown] is allowed only when the
  outer Array shape is proven and the unknown type argument stays explicit. It
  must never be displayed or consumed as Array[Object].
- Explicit RBS, YARD, bundled runtime, and validated extension types are
  contracts/evidence. Inferred bodies are checked against them rather than
  silently replacing them.
- Evidence exhaustion, solver bounds, incomplete dependencies, unsupported
  syntax, dynamic dispatch, and invalid source all resolve to Unknown with a
  machine-readable reason.

Never use confidence scores, naming conventions, popularity, observed runtime
behavior, the first convenient call site, a common superclass, Object, or an
arbitrary overload as a substitute for proof. A stable guess is still wrong.
Every concrete inferred type must be able to expose a bounded derivation from
source/signature facts to the final result.

## Pyrefly as a Reference, Not a Port

Pyrefly is a useful reference because its public architecture describes a
clear pipeline:

1. Compute module exports.
2. Lower a module to bindings containing static and flow information.
3. Solve bindings, using type variables as placeholders for recursive
   dependencies.

It deliberately favors complete module-level solving, module-level
incrementality, and parallel checking over a highly fine-grained demand solver.
It also exposes one type engine through a CLI and a language server, infers
unannotated return types, refines flow types, and offers first-use inference for
empty collections.

Adopt those principles where they fit:

- Separate binding collection from type solving.
- Represent joins and recursion explicitly instead of relying on traversal
  order.
- Make an editor-independent project/session API the shared entry point for
  the LSP and CLI.
- Recheck only invalidated modules/files and their semantic dependents.
- Prefer simple module/file-level units until profiling proves finer-grained
  incrementality is necessary.
- Infer checked unannotated method bodies by default.

Do not adopt first-use fixation as a Ruby heuristic. An empty collection gains
an element type only when complete, source-ordered flow evidence proves its
contents at that program point; otherwise its element remains Unknown.

Do not copy Python semantics or assume Python's module model. Ruby requires
first-class handling for reopened classes, include/prepend/extend, singleton
classes, dynamic dispatch, blocks/yield, RBS, refinements where supported, and
framework-generated declarations. Ruby Fast LSP must remain conservative when
those relationships are incomplete.

Official references reviewed for this goal:

- [Pyrefly architecture](https://github.com/facebook/pyrefly/blob/main/ARCHITECTURE.md)
- [Pyrefly project overview](https://github.com/facebook/pyrefly)
- [Pyrefly configuration and inference modes](https://pyrefly.org/en/docs/configuration/)
- [Pyrefly infer command](https://pyrefly.org/en/docs/autotype/)
- [Pyrefly coverage reporting](https://pyrefly.org/en/docs/report/)

## Current Checkpoint

Ruby Fast LSP already has valuable foundations:

- File-owned type facts and provenance in ruby-analysis core.
- Engine-owned MRO, method lookup, references, diagnostics, and deterministic
  per-file replacement.
- Literal and collection inference, local forward tracking, flow narrowing,
  method return inference, RBS lookup/substitution, and extension-declared
  structured types.
- Isolated engines per Gemfile-owned project.
- Bounded indexing, process resource governance, edit lifecycle tests,
  deterministic simulation, and real goshposh performance evidence.

The current implementation also exposes the next structural limits:

- RubyType has a small algebra and uses Unknown for several distinct meanings.
- Unknown currently absorbs unions, while bottom/unreachable, untyped/dynamic,
  unresolved, and invalid types are not represented separately.
- Class subtyping is simplified and is not consistently graph-aware.
- Parameters still default to Unknown in important paths, and some keyword,
  block, generic, self-type, and recursive-return behavior is incomplete.
- Flow tracking is mostly forward traversal rather than an explicit binding
  graph with joins and dependency-aware solving.
- Type inference is duplicated across TypeTracker, FactCollector helpers,
  completion helpers, and query fallbacks.
- ruby-analysis inference still contains tower-lsp Position, Range, and Url
  types in type_query.rs, violating the intended reusable boundary.
- The headless checker now shares the LSP cold-index lifecycle and domain
  diagnostics, but normalized inferred-type output and differential CLI/LSP
  type-parity coverage are not complete.

M0 must turn this checkpoint into measured evidence before large changes:
current score by category, Unknown frequency/reasons, wrong-type frequency,
diagnostic precision, cold/warm/edit costs, query latency, CPU, allocations,
and peak RSS.

### Implementation checkpoint — 2026-08-03

- The versioned M0 seed scorecard lives in
  `support/type_inference/scorecard.toml`; its schema, category allocation,
  proof-safety rules, and recorded outcomes are validated in normal tests.
- The explicit reporter currently measures **100/100 on 68 reviewed seed
  cases**, with all critical categories at 100% and no unexpected outcomes.
  The corpus includes one real `didChange` sequence that transitions a
  mutually recursive call chain from proven `String`, to base-free `Unknown`,
  to proven `Integer`, including chained hover and completion behavior. The
  minimum case-count gate is met, but `score_eligible = false`: normalized
  CLI/LSP type parity, reviewed real-project reductions, representative
  diagnostic precision, broader explanations, and the full performance matrix
  are not complete. Two
  positive diagnostic cases and four conservative-suppression cases provide
  an initial precision signal, not a representative precision claim.
  Supplemental breadth cases are deliberately worth zero points, so expanding
  the corpus did not inflate the original 100-point score. This diagnostic
  score must not yet be presented as achieving the product-level 9/10 goal.
- `ruby-analysis::core` now exposes a proof-carrying `TypeInferenceOutcome`
  that cannot represent `Proven(Unknown)`, plus versioned machine-readable
  Unknown reasons. Shared method-call inference retains
  `unknown_receiver`, `invalid_method_name`, `unresolved_method_return`,
  `incomplete_union_member`, or `unproven_recursive_cycle`. Source-ordered
  nonlocal reads retain `no_reaching_assignment`,
  `unresolved_assignment_value`, or `ambiguous_reaching_assignment` in a
  compact, sorted, file-owned evidence vector. Complete call expressions retain
  the same proof outcomes in a second compact vector: immediate RBS/constructor,
  union, invalid-name, and unknown-receiver results come from the shared call
  rule, while user-method results are finalized from the engine's existing
  navigation candidate, MRO chain, effective visibility, and solved return
  evidence. Engine queries expose the exact range result to both CLI and LSP;
  replacement removes it with the owning file. Existing type consumers still
  project failed proofs to
  `RubyType::Unknown`. File-owned
  method-return inference telemetry records proven/Unknown outcomes, stable
  reason-code counts, recursive component/method counts, solver iterations,
  and bound hits. Replacement removes stale telemetry with the owning file;
  Engine debug output and the checker aggregate it deterministically.
  The scorecard JSON publishes reason-code schema 2; broader local/flow reasons
  and bounded provenance chains remain M0 work.
- The editor-independent `CheckSession` and `ruby-fast-lsp check` command now
  run the same `IndexingCoordinator` cold-index lifecycle as LSP without
  starting an LSP service. Runtime selection, bundled core, runtime stdlib,
  Bundler/gems, signatures, extension inputs, project policy, umbrella-project
  discovery, isolated engines, fact collection, resolution, and semantic
  diagnostics are shared. A successful report sets
  `dependency_loading_complete = true`; a loader failure aborts rather than
  publishing absence claims from an incomplete universe. Explicit-file checks
  index the owning project but project only that file's diagnostics, while
  directory checks aggregate isolated projects deterministically. Human and
  versioned JSON output retain one-based UTF-16 ranges and diagnostic exit
  status. JSON schema 4 publishes sorted type subjects with an explicit kind:
  method returns retain canonical concrete labels or exact Unknown reason
  codes; nonlocal-read and call expressions retain exact engine-owned proven
  types or Unknown reasons, while other subjects are emitted only from
  concrete engine facts.
  Differential tests prove method
  returns, parameters, a value constant, local/instance/class/global
  assignments, and a multiline chain-boundary expression match LSP inlay
  labels and exact token positions. Additional tests prove unresolved receiver,
  unresolved return, and incomplete-union calls expose the same reason through
  CLI and hover; inlays remain silent where no concrete type is proven. Cold
  deterministic project collection now retains the same visitor-derived type
  facts as interactive indexing. YARD parameter types bind only to parameters
  that exist in the method syntax; an unmatched annotation remains a
  diagnostic and cannot become a concrete CLI subject. A
  black-box test proves the installed binary reports an engine-owned
  `wrong-arity` result. Exact method proof outcomes are retained only for
  project files and replaced with their owning file; dependency files keep
  counters without paying for the outcome map. Syntax diagnostics still
  require one
  checked-file reread/parse after cold indexing because the coordinator drops
  source text and does not yet persist parser diagnostics as domain facts; the
  CLI validates byte identity against the indexed content hash and fails if a
  source changed. Eliminating that duplicate syntax parse and adding edit
  lifecycle parity remain M6 work.
- Union receiver return inference is now proof-complete across the shared call,
  completion, and fact-collection paths. If any reachable member cannot prove
  the call result, the whole call remains Unknown; an LSP chained-call
  regression test prevents partial-union inference from returning a plausible
  concrete type.
- Multiline chained-call inlays now consume exact engine-owned expression facts
  at each chain boundary. The indexer records ordinary call-expression facts
  only where multiline-chain syntax can consume them, keeping the additional
  work bounded; the LSP adapter performs no independent inference. Proven
  boundaries such as `User.new` and `.profile` display their concrete
  intermediate types, while an unresolved boundary emits no placeholder or
  guessed hint. Both outcomes are represented in the reviewed scorecard.
- Receiver-less implicit and `self` calls with no first-pass receiver type now
  remain deferred until the engine's complete MRO is available instead of
  being prematurely frozen as `Unknown[unknown_receiver]`. Reopened method
  definitions produce an exhaustive return union only when every selected
  definition has a proven return. Explicit-receiver ambiguity additionally
  requires the unrestricted and visibility-allowed callee sets to be exactly
  equal; if private or otherwise inaccessible candidates are removed, the
  result remains `Unknown[unresolved_method_return]`. Differential CLI/LSP
  tests cover top-level implicit calls, public explicit calls, and the private
  fail-closed boundary, and the reviewed scorecard contains the same three
  semantic cases.
- Nonlocal-variable receiver proofs are now source ordered and owner aware in
  the shared engine query used by hover, completion, method-return chaining,
  navigation, signature help, references, call hierarchy, and diagnostics.
  Instance-variable owners match exactly; class variables share only the same
  namespace parts; globals remain process-wide. Index-time receiver resolution
  consults the collector's current source-ordered facts before the previously
  seeded engine, so a later Unknown write cannot leave a stale resolved
  reference or false missing-method diagnostic. Unknown assignment facts are
  retained as semantic proof barriers instead of being dropped during fact
  merging, and lookup chooses the latest write before deciding whether its
  payload is concrete. Assignment inlays query the exact write token and fail
  closed on conflicting producers. Reviewed regressions cover owner isolation,
  concrete-to-Unknown invalidation, inlay and hover output, completion,
  navigation, and diagnostic suppression for instance variables, plus
  navigation barriers for class and global variables.
- Source-ordered instance, class, and global variable reads now publish exact
  engine-owned expression facts. The CLI emits those facts with their exact
  ranges, and LSP hover consumes the same fact before any flow-query fallback;
  an exact Unknown therefore remains a proof barrier. Current-pass collection
  no longer consults stale same-file engine facts when no preceding write was
  observed. A balanced active-write stack models Ruby's RHS-before-target
  evaluation, so a read inside `@value = @value...` sees the previous write,
  not the pending target. Differential CLI/LSP and `didChange` regressions
  cover concrete reads, prior-value reads, and removal of a formerly concrete
  proof.
- Structured collection inference no longer replaces unresolved members with
  Object, widens mixed numerics to Numeric, or retains known members beside an
  unknown member. The proven outer shape remains `Array[Unknown]` or
  `Hash[Unknown, Unknown]`; exact known members remain normalized unions.
- Completion on a union receiver now intersects method availability across
  every member, reopened method returns require every selected definition to
  resolve, RBS intersections no longer select the first member, and the dormant
  confidence-based signature merge has been removed. These rules are shared
  domain behavior rather than editor-side filtering. Distinct reopened method
  definitions retain their own range-owned return facts deterministically,
  while an inferred body fact cannot override an explicit contract attached to
  the same definition.
- Direct and same-file mutually recursive method returns now use a bounded,
  deterministic SCC least-fixed-point solver with a private bottom value that
  cannot enter `RubyType` or public facts. Compact return equations are emitted
  during the existing method-body traversal, grouped in stable namespace and
  method order, and solved synchronously without another Prism parse or AST
  walk. A concrete terminating base can prove the component; a base-free
  component, unresolved base, incomplete reopened definition, or
  non-converging equation remains `unproven_recursive_cycle`. Explicit
  `return` paths participate in a method's inferred return union, fixing two
  old guard cases that had omitted the reachable bare-return `NilClass` result.
  Cross-file recursive SCC solving remains future work.
- `RubyType` union normalization now uses structural ordering instead of
  allocating and sorting debug strings. Canonical Boolean and inferred union
  ordering therefore follow one deterministic path.
- Same-file equation dependencies now survive straight-line local aliases such
  as `value = helper; value` as private solver terms rather than public
  `Unknown`. Yielding/proc result proofs run before ordinary callee-equation
  capture, preserving proven block-return inference. Dependency aliases inside
  branches and loops are deliberately cleared until the private terms have a
  real join model; those cases remain Unknown instead of borrowing a term from
  another control-flow path. Forward mutual dependencies are also retained
  when deterministic parallel project collection has pre-registered files but
  has not inserted their direct method facts yet; a missing target remains an
  unresolved dependency, while a target present in the completed equation set
  participates in the SCC. Two core regression tests and CLI telemetry/parity
  tests cover that batch lifecycle.
- Canonical RBS conversion now uses proof-first union construction for unions,
  optionals, and booleans in both inference and signature indexing. Exact
  `Array[T]` and `Hash[K, V]` arities take allocation-bounded direct paths;
  malformed arity, intersections without a supported proof rule, and unions
  containing untyped/Unknown evidence fail closed. Specialized Boolean,
  optional, and single collection-member constructors preserve structural
  ordering without invoking the general sort path.
- The next M0 work is to extend normalized CLI/LSP parity across the remaining
  signature/diagnostic query projections, add machine-readable Unknown
  explanations for the remaining local-flow expressions, and broaden
  reviewed diagnostic precision, followed by
  release-build base-versus-candidate cold/warm/edit/query resource baselines
  for the expanded solver, proof evidence, and telemetry path.
- The first non-claim-eligible release scorecard timing is recorded in
  `support/performance/type-inference-scorecard-m0-seed-2026-08-03.json`:
  seven warm process runs have a **0.35 s median**, and one warm sample reports
  **100,352,000 bytes maximum RSS**. It is a candidate-worktree reference, not
  a base-versus-candidate non-regression result; the full M0 performance matrix
  remains required.
- The expanded 50-case workload has its own non-comparable candidate reference
  in `support/performance/type-inference-scorecard-m0-expanded-2026-08-03.json`:
  seven warm release process runs have a **0.47 s median**, the scorecard test
  itself reports **0.19 s**, and one warm sample reports **100,974,592 bytes
  maximum RSS**. The older 15-case median is not used as a baseline for this
  larger workload; an alternating base-versus-candidate matrix is still
  required.
- A first same-fixture base-versus-candidate release comparison is recorded in
  `support/performance/type-inference-single-file-base-vs-candidate-2026-08-03.json`.
  Across seven candidate/base pairs, the 242-byte semantic-pass fixture has a
  **75.69 ms candidate median versus 76.07 ms base (-0.50%)**. Seven process
  samples show **73,875,456-byte candidate median RSS versus 75,677,696-byte
  base (-2.38%)**. This clears the focused 3% median wall gate, but one
  candidate RSS outlier and the absence of workspace/edit/query profiles mean
  the full M0 performance matrix remains open.
- The recursive-solver scorecard comparison is recorded in
  `support/performance/type-inference-recursive-solver-scorecard-2026-08-03.json`.
  Its six warm pre/post process samples measure **0.445 s baseline versus
  0.455 s candidate (+2.25%)** while the score rises from 93 to 100. This
  clears the focused 3% median wall gate for the recursive slice only; it does
  not replace the required workspace, edit, query, allocation, or peak-RSS
  matrix.
- The cumulative proof-evidence single-file comparison is recorded in
  `support/performance/type-inference-proof-evidence-single-file-base-vs-candidate-2026-08-03.json`.
  Seven alternating release pairs measure a **74.38 ms candidate median versus
  75.02 ms base (-0.85%)**, with **73,908,224-byte candidate median RSS versus
  75,022,336-byte base (-1.49%)**. This clears the focused 3% median wall gate
  after adding deterministic SCC solving, proof telemetry, file-owned exact
  outcomes, and CLI type projection. It remains a 242-byte single-file result,
  not the required workspace/edit/query/allocation/peak-RSS acceptance matrix.
- A larger cumulative comparison is recorded in
  `support/performance/type-inference-chained-call-large-file-base-vs-candidate-2026-08-03.json`.
  Fifteen alternating release pairs on the 225,569-byte, 5,095-line
  `consignments.rb` fixture measure a **108.88 ms candidate file-processing
  median versus 107.66 ms base (+1.13%)**. Seven independent process-resource
  pairs measure **0.84 s candidate wall versus 0.85 s base (-1.18%)**,
  **0.83 s CPU versus 0.84 s (-1.19%)**, and **79,691,776-byte median RSS
  versus 79,052,800 bytes (+0.81%)**. These final-state measurements include
  the batch/SCC proof-term fix and clear the focused 3% gates. The
  replace-and-resolve subphase is still 0.53 ms slower at the median, and the
  workspace/edit/query/allocation/peak-RSS matrix remains open.
- The cold deterministic project-batch parity comparison is recorded in
  `support/performance/type-inference-cold-project-parity-base-vs-candidate-2026-08-03.json`.
  Fifteen alternating release pairs on the six-file
  `goshposh_helpers_goto` fixture measure a **94.23 ms candidate wall median
  versus 92.51 ms base (+1.86%)** and **94.02 ms candidate CPU versus 92.35 ms
  (+1.81%)**. Seven process-resource pairs measure **33,849,344-byte candidate
  median RSS versus 33,538,048 bytes (+0.93%)**. Retaining visitor-derived
  proof facts adds 0.14 ms to assembly and 0.51 ms to replacement at the
  median, while end-to-end wall, CPU, and RSS remain within the 3% gates. This
  covers one fixed cold project collection path, not warm cache, edit/query
  p95, allocations, or the full goshposh RSS ceiling.
- The focused interactive nonlocal-proof budget evidence is recorded in
  `support/performance/type-inference-nonlocal-proof-query-budget-2026-08-03.json`.
  Seven release processes with 500 observations per query report median p95s
  of **1.12 ms edit**, **23.79 us completion**, **11.75 us hover**, **18.79 us
  definition**, **5.11 ms references**, and **2.29 us diagnostics**, with a
  **588.79 ms** cold-index median and **6.0 MiB** engine heap. A separate
  `--check-budgets` run passes every fixed production gate. The untouched base
  profiler could not validate references on the same minimal fixture and
  indexed a much smaller semantic universe, so its partial timings are kept
  only as orientation rather than a like-for-like no-regression claim. The
  full warm workspace/edit/allocation/goshposh matrix remains open.
- The exact nonlocal-read parity slice is recorded in
  `support/performance/type-inference-nonlocal-read-parity-2026-08-03.json`.
  Fifteen release passes over the 225,569-byte `consignments.rb` fixture
  measure a **105.12 ms file-processing median** versus the immediate
  pre-change artifact's **108.88 ms (-3.45%)**. Seven processes with 500 query
  observations each keep edit, completion, hover, references, and diagnostics
  median p95 changes within the 3% gates; definition improves, and every fixed
  production budget passes. A retained older hashed profiler indexed a
  materially smaller semantic universe and was excluded rather than presented
  as a false alternating baseline. Cold-process samples changed semantic
  producer/cache identity, so this slice relies on the same-fixture semantic
  pass for acceptance and leaves the full warm/allocation/goshposh matrix open.
- The nonlocal-read Unknown-explanation slice is recorded in
  `support/performance/type-inference-nonlocal-read-unknown-reasons-2026-08-03.json`.
  Fifteen alternating hashed release-binary pairs isolate reason retention on
  the same 225,569-byte fixture: the candidate improves median file processing
  by **2.33%**, visitor time by **2.13%**, and replacement by **2.33%** versus
  the retention-disabled binary. Seven fresh 500-iteration query processes
  keep every median p95 delta within 3%, retain **6.1 MB** engine heap, and pass
  every fixed production budget. Non-alternating samples that drifted beyond
  the focused threshold were rejected rather than used as acceptance evidence.
- The general call-expression outcome slice is recorded in
  `support/performance/type-inference-call-expression-outcomes-2026-08-03.json`.
  Fifteen alternating hashed release-binary pairs on the same 225,569-byte
  fixture measure a **115.49 ms candidate file-processing median versus
  114.26 ms baseline (+1.08%)**, with visitor time at **+0.71%**. Reusing the
  navigation resolver's MRO and effective-visibility result plus a linear
  sorted-outcome merge keeps the focused end-to-end result inside the fixed 3%
  gate. Replace-and-resolve remains **0.48 ms / 4.26%** slower and is explicit
  follow-up work. Fourteen fresh 500-iteration query processes pass every fixed
  production budget with **6.1 MB** engine heap; completion, hover, definition,
  and diagnostics median p95 deltas remain within 3%, while edit (**+4.27%**)
  and references (**+4.03%**) remain above the comparison threshold. This slice
  is therefore semantically accepted but not yet a full performance
  non-regression result. A per-call TypeFact design that increased one exact pass from
  117.83 ms to 206.95 ms and a pass-local hash cache at +5.13% were both
  rejected and removed.
- The reopened-call finalization increment is measured separately in
  `support/performance/type-inference-reopened-call-proof-2026-08-03.json`
  against exact pre-change binaries compiled from the same worktree state.
  Fifteen alternating large-file pairs put file processing at **+0.74%**,
  visitor time at **+0.57%**, and replace-and-resolve at **+1.29%**. Fourteen
  alternating 500-iteration query pairs keep edit (**+0.96%**), completion
  (**+1.65%**), hover (**-0.53%**), definition (**+0.12%**), references
  (**-1.32%**), diagnostics (**-0.94%**), and cold indexing (**+0.37%**)
  inside the 3% incremental gate; both binaries load the same 134-file,
  **6.1 MB** semantic universe and every fixed production budget passes. This
  proves the increment is performance-safe but does not close the broader
  call-expression and goal-wide performance work above.
- Five fresh release `ruby-fast-lsp check --format json` processes per fixture
  produced byte-identical output for three focused CLI fixtures, including the
  exact nonlocal-read and call-expression Unknown reason codes plus the newly
  proven reopened explicit-call union; the commands, exact stdout SHA-256
  values, and limitations are recorded in
  `support/type_inference/cli-determinism-2026-08-03.json`. This supplements
  the scorecard's repeatable-type, canonical-union, and edit-lifecycle cases;
  it is not yet a broad worker-schedule determinism claim.
- The current bounded-four-thread correctness gate passes **1,452/1,452
  non-ignored root tests**
  (one existing ignored test), **444/444 ruby-analysis tests**, and **71/71
  scorecard cases** with zero unexpected outcomes. The non-root workspace
  suite, `cargo check --workspace`, release build, and all **58/58** VS Code
  adapter tests also pass. Default maximum-concurrency retries exposed two
  unrelated external-process timing flakes (a Standard fixer timeout and a
  runtime-version probe output race); each passed alone, and bounded
  concurrency passed the complete suite. This repository-level flake still
  needs stabilization before the final completion claim. Every Rust file
  changed by this goal is
  rustfmt-clean; repository-wide rustfmt has the same pre-existing drift at the
  base commit and remains an open repository gate rather than being silently
  reformatted as part of this inference change.

### Implementation checkpoint — 2026-08-05

- Two in-flight equation-reuse failures were fixed. The direct-facts seed
  replacement in `src/indexer/file_processor.rs` staged facts with
  `inference: Default::default()`, so every edit transiently wiped the
  file's solved inference and marked the method-return solver dirty; the
  seed write now preserves the previous engine-owned `InferenceEvidence`
  (new `AnalysisEngine::inference_evidence_in_file` accessor) because the
  seed pass does not own inference. An unchanged-equation edit now runs
  **zero** equation solves, a changed recursive base runs **exactly one**,
  and a base-free edit invalidates to `Unknown[unresolved_method_return]`.
  The engine test that handed different outcomes with identical equations
  contradicted the documented reuse contract (equations unchanged => the
  previous project-solved outcomes stay authoritative) and now changes the
  equations together with the outcomes while still proving file-owned
  evidence replacement.
- Normalized CLI/LSP diagnostic parity now covers the remaining
  engine-owned projections with eight differential tests: `unresolved-method`
  (including the Levenshtein suggestion and the conservative `User.new`
  fact), `unresolved-constant`, `missing-kwarg`, `yard-unknown-param`,
  `yard-rbs-mismatch` (bundled RBS contract conflicts, e.g. `String#length`
  is `Integer` in RBS), `unresolved-require`, parser syntax errors, and a
  multi-diagnostic fixture proving identical sorted sets. Two findings came
  out of the fixtures: `User.new` is flagged unresolved by both CLI and LSP
  (a parity-consistent but suspect `Class#new` resolution gap), and the CLI
  normalizes syntax diagnostics by position/message while LSP preserves
  prism error order, so syntax parity is asserted as full sorted sets.
- The scorecard corpus grew to **77 cases (m0-seed-15)** with five reviewed
  zero-point diagnostic-parity coverage cases; the explicit reporter still
  measures **100/100** with `score_eligible = false` and every critical
  category at 100%.
- The full root suite passes **1,462/1,462** non-ignored tests
  (one existing ignored test), **444/444 ruby-analysis tests**, the
  scorecard reporter (ignored) passes with 77 cases, the black-box
  `tests/check_cli.rs` binary test passes, and the extension harness
  test passes. No performance artifacts were produced for this
  parity-only slice; the alternating release matrix and the remaining
  Unknown-explanation, precision, and real-project work stay open.
- Next M0 work: machine-readable Unknown explanations for the remaining
  local-flow expressions, reviewed diagnostic precision with reduced
  real-project fixtures, and the complete alternating release-build
  baseline/candidate cold/warm/edit/query/allocation/peak-RSS matrix.

## Correct Architecture

### Layer ownership

| Layer | Owns | Must not own |
| --- | --- | --- |
| ruby-analysis::core | Ruby type algebra, type IDs/variables, constraints, ranges, facts, provenance, diagnostics | Prism traversal, LSP types, CLI formatting |
| ruby-analysis::indexer | Prism parsing, scope-aware AST traversal, binding/fact/candidate emission | Workspace truth, LSP protocol, terminal output |
| ruby-analysis::inference | Constraint generation rules, flow environments, narrowing, joins, overload/generic solving, inferred signatures | LSP/CLI UX, persistent workspace ownership |
| ruby-analysis::engine | File registry, semantic graph, facts, dependency/invalidation graph, deterministic queries and stored solved results | Prism traversal, tower-lsp types, output formatting |
| reusable check session | Editor-agnostic project loading and orchestration of indexer, inference, engine, and diagnostics | LSP transport and terminal rendering |
| ruby-fast-lsp LSP adapter | Document lifecycle, scheduling, protocol conversion and publication | Type rules or a second diagnostic policy |
| ruby-fast-lsp check adapter | CLI arguments, exit codes, human/JSON rendering | Type rules or a second project model |

The reusable check session may be a module inside ruby-analysis or a small
crate above it. Its dependency direction must remain acyclic. Inference asks
semantic questions through a narrow trait implemented by an engine query or
immutable snapshot; the engine does not become an AST walker.

### Hard modularity gates

- ruby-analysis must not depend on tower-lsp after the boundary migration.
- Domain APIs use SourceFileId, TextRange, Ruby names/FQNs, RubyType, and
  domain diagnostics only.
- The CLI must work with no LSP client, editor process, or protocol objects.
- The LSP and CLI must call the same project/session check API.
- There is one method/MRO/visibility/ambiguity policy:
  AnalysisQuery resolution.
- There is one file replacement lifecycle:
  register_file, collect, replace_facts, resolve/check.
- There is one source precedence policy for explicit signatures, generated
  declarations, inferred facts, and unknowns.
- Parsing and scope traversal remain separate from solving. New inference must
  not introduce a second Prism parse or full AST walk per feature.
- Public APIs expose domain views/results, not engine stores or hash maps.

### Target pipeline

    source + RBS + extension/runtime inputs
                    |
                    v
        one offset-preserving Prism parse
                    |
                    v
        scope-aware facts and typed bindings
                    |
                    v
       engine graph + inference query snapshot
                    |
                    v
      bounded binding/constraint solve to fixpoint
                    |
                    v
      solved type facts + semantic diagnostics
                    |
           +--------+--------+
           |                 |
           v                 v
       LSP adapter       check CLI adapter

The indexer may emit binding IR beside ordinary facts during its existing
recursive traversal. The solver consumes that IR after declarations and graph
relationships are available. Results enter the existing file-owned fact and
diagnostic lifecycle; there is no parallel semantic store.

## Type-System Direction

### A precise type algebra

Evolve RubyType or its replacement to distinguish at least:

- Unknown: insufficient static evidence, with a reason.
- Untyped/dynamic: an explicit escape hatch such as RBS untyped.
- Never/bottom: an expression or path that cannot produce a value.
- Nil, booleans, literals, named instance types, class/module objects, and
  self types.
- Normalized unions and intersections where RBS or narrowing needs them.
- Generic applications with type arguments, not special-case Array/Hash
  vectors.
- Tuple, record/shape, callable/block/proc, and type-variable forms needed by
  supported Ruby/RBS semantics.

Type construction must be canonical, deterministic, and cheap to compare.
Normalize unions without debug-string sorting. Bound union width and recursive
type depth with explicit overflow-to-Unknown rules and counters. Never silently
substitute Object or another wider type for an invalid, incomplete, or
over-budget inference.

Unknown, Untyped, and Never must not share behavior:

- Unknown blocks unsafe claims but retains evidence and may be refined later.
- Untyped permits gradual interaction without pretending a concrete type was
  inferred.
- Never disappears at reachable joins and supports definite-return reasoning.

### Bindings, constraints, and flow

Represent definitions, uses, calls, exports, and anonymous checked expressions
as stable binding identities scoped by source file, lexical scope, and byte
offset. Bindings may depend on other bindings or external semantic queries.

Required flow behavior includes:

- Source-ordered local variables with hard method/class scope boundaries and
  block capture.
- Phi-style joins for if/unless, case/case-in, rescue/else/ensure, and loops.
- Narrowing for nil/truthiness, is_a?/kind_of?, case equality, respond_to?
  only when defensible, pattern matching, and terminating guards.
- Correct invalidation after assignment, mutation, aliasing, or calls that make
  a refinement unsafe.
- Bounded loop and recursive-method fixpoints with deterministic proof
  convergence.
- Explicit return, implicit last expression, next/break values, raise, yield,
  super, blocks, lambdas, procs, and forwarding arguments.
- No dependence on file traversal, hash iteration, or Rayon scheduling order.

Recursive and mutually recursive bindings use placeholders/type variables and
solve by a bounded SCC/fixpoint policy. Hitting a bound yields Unknown with an
explicit solver-bound reason and telemetry; it must not hang, explode a union,
or publish a widened concrete result.

### Calls, signatures, and generics

Method-call inference must compose the existing engine-owned lookup chain with:

- Instance, singleton, inherited, included, prepended, extended, and reopened
  methods.
- Public/protected/private visibility and ambiguity.
- Positional, optional, rest, keyword, keyword-rest, block, forwarding, and
  Ruby options-hash compatibility.
- Overload selection by argument shape and type.
- Generic type-variable solving from receiver, arguments, block parameters,
  block result, and expected result when available.
- RBS self, instance, class, interface, alias, union, intersection, optional,
  tuple, record, proc, top, bottom, and untyped forms used by the corpus.
- Constructor/new semantics, attr readers/writers, alias methods, super, yield,
  and common enumerator/container propagation.

An incomplete receiver hierarchy or ambiguous lookup must fail closed:
navigation may retain diagnostic-free candidates, but type checking must not
claim a missing method or select an arbitrary overload.

### Interprocedural and cross-file inference

- Infer unannotated checked method returns from every reachable return path.
- Infer parameter contracts from explicit RBS/YARD/validated generated
  signatures. Call-site evidence may constrain only a closed, local callable
  when the engine proves the call set is exhaustive and has no external entry;
  observed calls must never define a public/open-world method parameter type.
- Propagate public method returns, constants, and attribute types across files.
- Handle reopened owners and conflicting declarations deterministically.
- Detect recursive call groups and stabilize them without whole-workspace
  iteration after every edit.
- Treat inferred public signatures as semantic exports. A changed export
  invalidates its dependents; a body-only edit does not.
- Keep extension/Rails/generated facts in the same graph and provenance
  lifecycle. Framework-specific inference stays in extensions, while the core
  solver consumes generic structured facts.

Explicit annotations win as contracts. Inferred implementations are checked
against them. A mismatch produces a type diagnostic; it does not silently
replace the declared contract.

## Standalone Type Checker

The supported headless entry point is:

    ruby-fast-lsp check [PATH ...]

Initial CLI contract:

- With no path, check the current Gemfile-owned project.
- Accept files or directories while using the same project ownership, source
  policy, runtime, load paths, gem, stdlib, RBS, and extension inputs as LSP.
- Default to stable human-readable diagnostics with file, range, severity,
  error code, message, and relevant type evidence.
- Support a versioned JSON output for agents and CI.
- Exit 0 when no enabled errors exist, 1 when type errors exist, and a distinct
  nonzero code for configuration/internal failures.
- Offer a summary containing files checked, errors/warnings, elapsed time,
  inferred/Unknown counts, cache/incremental reuse, and peak memory when
  available.
- Produce deterministic diagnostic ordering by project, file, range, and code.
- Never mutate source during check.

The first phase does not need automatic annotation insertion. Keep inferred
annotation writing and stub generation as later tools over the same solved
types, following Pyrefly's separation between check, infer, and stub generation.

CLI/LSP parity is mandatory:

- Given byte-identical inputs and configuration, both surfaces produce the same
  domain diagnostics and solved exported types.
- LSP may publish only open-document diagnostics while the CLI prints the full
  project result; that is a projection difference, not a semantic difference.
- Human text, JSON, LSP ranges, and severities are adapters over stable
  diagnostic codes and TextRange values.

## The 100-Point Type Inference Scorecard

Create a checked-in, machine-readable Ruby inference corpus. Every assertion
has a category, input project, query site, expected canonical type or expected
diagnostic, and whether a conservative Unknown is permitted. The harness emits
the total and per-category score plus misses, wrong concrete types, unexpected
Unknowns, and false-positive diagnostics.

| Category | Points |
| --- | ---: |
| Literals, operators, interpolations, and collections | 10 |
| Locals, assignments, scopes, captured variables, and attributes | 10 |
| Branches, guards, pattern matching, rescue, loops, and reachability | 15 |
| Calls, constructors, dispatch, visibility, MRO, mixins, and super | 15 |
| Methods, parameters, returns, blocks, yield, proc/lambda, and forwarding | 15 |
| RBS types, overloads, generics, substitution, and annotation checking | 15 |
| Constants, reopened definitions, cross-file propagation, and invalidation | 10 |
| Supported generated/framework/runtime facts, including Rails and JRuby | 5 |
| Lifecycle determinism, malformed code, CLI/LSP parity, and explanations | 5 |
| **Total** | **100** |

Scoring rules:

- Exact canonical type, including an exhaustive normalized union: full credit.
- Supported site returning Unknown: no credit.
- Wrong concrete type: no credit and a correctness failure, even if the total
  remains above 90.
- A wider superclass, Object, a partial union with an unproven branch, or a
  heuristic call-site type is a wrong concrete result, not partial credit.
- Correctly refusing a declared dynamic/non-goal boundary is tested for safety
  but does not inflate the accuracy score.
- A diagnostic assertion requires correct code, range, severity, and relevant
  expected/actual types.
- Fixtures must not be added or reweighted merely to raise the score. Changes
  require a review note explaining the semantic reason.

The corpus must include small table-driven cases, multi-file projects,
edit/reindex sequences, RBS overlays, Rails/extension facts, and reduced cases
from real open-source Ruby projects. Keep a separate reviewed diagnostic corpus
to measure precision rather than only inference coverage.

## Definition of 9/10

The type-inference goal is complete only when all of the following are true:

1. The scorecard is at least **90/100** overall.
2. Every category scores at least **85%** of its available points.
3. There are zero known wrong-concrete-type results in the supported corpus.
4. The deterministic simulation and reviewed real-project corpora have zero
   known false-positive type diagnostics. A concrete diagnostic requires a
   complete proof, not a confidence threshold.
5. Unknown results at supported sites are below 10% overall and carry a
   machine-readable reason.
6. CLI and LSP domain diagnostics/types are byte-for-byte equivalent after
   normalization on parity fixtures.
7. Hover, inlay hints, completion, navigation, signature help, and diagnostics
   consume the shared solved types on their acceptance cases.
8. ruby-analysis has no tower-lsp dependency, and the check command runs
   without initializing an LSP service.
9. Identical inputs produce identical exported types, diagnostics, and
   semantic fingerprints across repeated runs and worker schedules.
10. Every performance and memory gate below passes.

The remaining 1/10 may contain explicitly documented dynamic Ruby boundaries
such as string eval, arbitrary runtime reflection, data-dependent
method_missing, native-extension behavior with no static declarations, and
unbounded metaprogramming. These boundaries must degrade to explained Unknown
without false diagnostics.

## Performance and Incrementality Policy

### Measure before design changes

M0 records release-build baselines on:

- The checked-in inference conformance corpus.
- A medium deterministic multi-file fixture.
- The existing real goshposh projects and umbrella workspace.
- Cold process, warm persistent cache, ready-project query, body-only edit,
  exported-signature edit, RBS edit, and extension-fact edit scenarios.

Record wall time, user/system CPU, allocations where available, peak/end RSS,
parse count, AST traversal count, binding count, constraint count, solver
iterations, proof failures, Unknown reasons, invalidated files, recomputed
bindings, cache hits, diagnostics, and semantic fingerprints.

Use symbolized profiles to choose hot paths. Do not add a general fine-grained
query framework, whole-workspace solver, or new retained cache without evidence
that the simpler file/module-level design cannot meet the budgets.

### Non-regression gates

- Existing active-buffer parsing and same-file navigation remain within
  **500 ms p95**.
- Existing project navigation, dependency navigation, semantic readiness,
  all-project completion, and status budgets in AGENTS.md remain in force.
- A ready project's body-only edit must not synchronously check closed files or
  fan out through all semantic dependents.
- Body-only edit median CPU and p95 latency may not regress by more than 3%
  versus the recorded M0 baseline outside measured noise.
- Cold and warm project indexing/checking median wall time and user CPU may not
  regress by more than 3% versus M0.
- Ready-project hover, completion, definition, inlay-hint, and diagnostic query
  p95 may not regress by more than 3% versus M0.
- Warm two-project goshposh peak RSS must remain at or below
  **1,776,846,438 bytes**.
- No new cache may be unbounded. Retained cache weight, entry limit, eviction,
  identity, and invalidation require focused tests and profiler evidence.
- The ordinary source pass parses each file once and performs one primary
  scope-aware traversal. Type solving may revisit compact bindings, not Prism
  trees, until profiling proves otherwise.
- Union width, recursive type depth, SCC iterations, diagnostics per file, and
  explanation depth have explicit bounds and telemetry. Crossing an inference
  bound produces Unknown, never a guessed or widened concrete type.
- Equivalent cold/warm results have identical normalized type/diagnostic
  fingerprints.

Compare release builds on the same machine, dataset, runtime, lockfiles, cache
state, resource-governor settings, and build fingerprints. Use alternating
baseline/candidate runs. Treat a result outside the noise envelope or above the
hard 3%/RSS limits as rejected until redesigned.

### Incremental policy

- Cache solved results by exact file content plus semantic dependency/export
  identity, not timestamps.
- A body-only change replaces that file and refreshes the current/open
  projection.
- A changed exported signature invalidates only files/bindings that depend on
  it. Closed-file work stays outside the didChange critical path.
- RBS, superclass, mixin, method visibility, extension, runtime, and gem changes
  invalidate the exact semantic products they affect.
- Cancellation cannot publish stale types or diagnostics into a newer document
  version or project generation.
- Multi-root engines remain isolated; shared immutable products never share
  project-specific solved state.

## Diagnostics Policy

Start with high-value diagnostics directly enabled by solved types:

- Argument and keyword type mismatch.
- Return type mismatch.
- Assignment/constant/attribute contract mismatch.
- Invalid receiver or missing method only when the full lookup chain is known.
- Incompatible block parameter/result.
- Invalid override where RBS/Ruby contracts are complete.
- Unreachable or impossible branches supported by Never and narrowing.

Each diagnostic has a stable code, primary range, expected/actual type,
provenance, and bounded explanation chain. The CLI and LSP select severity and
format independently of the rule.

Do not emit a type error when:

- The necessary superclass/mixin/extension edge is unresolved.
- Dispatch is genuinely ambiguous.
- The value is explicitly Untyped.
- A dynamic boundary is outside the supported contract.
- The engine cannot prove the error without guessing.

Unknown suppresses unsupported claims; it must not erase existing syntax,
definite arity, or other independent diagnostics.

## Milestones

### M0 — Baseline, corpus, and observability

- Freeze scorecard schema and representative fixtures.
- Record current overall/per-category score and reviewed diagnostic precision.
- Add Unknown reason/provenance and inference counters without changing
  semantics.
- Record cold/warm/edit/query CPU, latency, allocation, and RSS baselines.

Exit: a machine-readable report shows exactly why the current system is below
9/10 and establishes the non-regression comparison.

### M1 — Domain boundary and type algebra

- Remove tower-lsp types/dependency from ruby-analysis.
- Separate Unknown, Untyped, and Never.
- Add canonical generic, literal, callable, tuple/record, union/intersection,
  self, and type-variable forms needed by the corpus.
- Centralize normalization, subtyping/assignability, display, provenance, and
  proof validation, with overflow and incomplete evidence resolving to Unknown.
- Add the reusable project/check session facade and a skeletal check command.

Exit: both CLI and LSP can query the same domain types, and the new algebra has
focused invariant/property tests with no performance regression.

### M2 — Binding IR and flow solver

- Emit stable bindings and dependencies during the existing AST traversal.
- Implement joins, narrowing, reachability, loops, rescue, pattern matching,
  closure capture, and deterministic bounded fixpoints.
- Remove equivalent ad-hoc rescans/fallback inference after parity tests.

Exit: local/flow categories meet their thresholds; one parse/traversal and edit
budgets remain intact.

### M3 — Calls, methods, blocks, and generics

- Use the sole engine lookup policy for receiver dispatch.
- Solve arguments, keywords, blocks, yields, returns, super, forwarding,
  overloads, and generic substitutions.
- Complete the supported RBS type forms and check inferred bodies against
  declared contracts.

Exit: call/method/RBS categories meet their thresholds with no false missing
method claims from incomplete lookup.

### M4 — Cross-file solving and precise invalidation

- Publish inferred signatures/constants/attributes as semantic exports.
- Solve recursive method/file groups with bounded deterministic SCC logic.
- Track dependencies and distinguish body-only from exported-type changes.
- Prove edit, RBS, reopen, mixin, runtime, and extension invalidation.

Exit: multi-file category meets its threshold; body-only editing remains
bounded and cross-file changes converge without stale facts.

### M5 — Use types consistently

- Route hover, inlay hints, completion, navigation, signature help, and semantic
  diagnostics through shared solved results.
- Remove conflicting feature-local inference or retain it only as a tested
  adapter fallback during migration.
- Add bounded type explanations and stable diagnostic codes.

Exit: consumer acceptance and CLI/LSP parity tests pass.

### M6 — Production CLI

- Complete ruby-fast-lsp check project/file discovery and configuration.
- Add human and versioned JSON output, deterministic ordering, exit codes, and
  summary statistics.
- Add black-box installation and CI usage tests.

Exit: the type checker works headlessly on real projects and returns the same
domain results as the LSP.

### M7 — 9/10 and performance acceptance

- Reach 90/100 with every category at or above 85%.
- Run deterministic simulations and reviewed real-project precision checks.
- Run alternating M0/candidate cold, warm, edit, query, and goshposh profiles.
- Preserve the fixed RSS ceiling, interactive budgets, engine isolation, and
  deterministic fingerprints.

Exit: every item in Definition of 9/10 and the local completion gate passes.

## Test and Acceptance Matrix

- Table-driven type algebra, normalization, assignability, proof, and
  proof-failure-to-Unknown tests.
- Inline expression/type/diagnostic tests for every scorecard assertion.
- Multi-file binding, recursive call, reopen, mixin, and overload tests.
- FakeEditor open/change/save/close tests for stale-type removal and parity.
- CLI black-box tests for output, ordering, exit codes, malformed source,
  configuration failures, and multi-root ownership.
- Differential tests comparing CLI domain JSON with normalized LSP results.
- Determinism tests across insertion and worker scheduling order.
- Simulation tests with generated call graphs, inheritance, edits, and an
  oracle for expected types/diagnostics.
- RBS precedence, generic substitution, overload, edit, and parse-failure tests.
- Extension/Rails/JRuby structured fact and invalidation tests.
- Unknown safety tests for incomplete ancestors, dynamic send, method_missing,
  eval, ambiguous calls, and missing external declarations.
- Performance tests for one parse/traversal, bounded solver iterations, precise
  invalidation, cancellation, cache bounds, and RSS.
- Real project review of inferred types and diagnostics before claiming score.

Every semantic slice follows red-green-refactor: first add the smallest
scorecard or regression case, prove it fails for the intended reason, implement
the reusable rule, prove all consumers and performance gates, then remove any
superseded fallback.

## Explicit Non-Goals for 9/10

- Executing user Ruby code, gemspecs, Rails applications, or native extensions
  to discover types.
- Parsing or interpreting arbitrary string eval/class_eval/module_eval.
- Guessing through data-dependent send, const_get, method_missing, or runtime
  reflection.
- A second semantic engine for the CLI.
- A type algorithm inside LSP handlers, VS Code, or framework extensions.
- Whole-workspace rechecking on each keystroke.
- Unbounded union growth, recursion, caches, diagnostic output, or explanation
  graphs.
- Automatic annotation/source rewriting in the initial check command.
- Claiming Pyrefly's Python throughput or conformance numbers as Ruby Fast LSP
  acceptance evidence.

## Local Completion Gate

Before the final 9/10 claim:

    cargo fmt --all -- --check
    cargo test
    cargo test --workspace --exclude ruby-fast-lsp
    cargo build --release
    npm --prefix editors/vscode/vsix test
    ./editors/vscode/create_vsix.sh --current-platform-only
    ruby-fast-lsp check <checked-in-conformance-corpus> --format json

Also run the recorded inference performance suite and the existing multi-root
goshposh cold/warm/edit/query profiles. Preserve exact repository, binary,
machine, runtime, lockfile, dataset, cache, and governor fingerprints with the
score report, semantic fingerprints, latency/CPU/RSS evidence, and accepted or
rejected design decision.

Do not mark the goal complete because individual examples look good. Completion
requires the score, precision, parity, modularity, determinism, performance,
memory, packaging, and real-project gates together.
