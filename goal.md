# Goal: Proof-First Parameter-Dependent Callable Bodies

Status: **Complete — accepted 2026-08-13**

## Implementation Progress

- [x] Phase 0 — pin the proof domain, fixed bounds, neutral RED fixtures, and
  exact pre-implementation release baseline.
- [x] Phase 1 — introduce one compact AST-free callable-body summary and
  stable explained-Unknown outcomes.
- [x] Phase 2 — give local callables bounded flow identities and constant-held
  callables ordinary file-owned fact lifecycle semantics.
- [x] Phase 3 — instantiate direct `.call` bodies from proven arguments.
- [x] Phase 4 — reuse the same instantiation in the higher-order call solver.
- [x] Phase 5 — complete captures, aliases, flow, shapes, unions, and
  lambda/proc arity semantics within the accepted bounds.
- [x] Phase 6 — prove edit/reindex/file-order determinism and parity across
  hover, inlay hints, completion, diagnostics, chained dispatch, and CLI.
- [x] Phase 7 — complete architecture/docs/scorecard updates, full tests, and
  alternating release performance/memory evidence.

## Completion Evidence

- One AST-free callable-body summary and one evaluator now serve direct
  `.call` and higher-order `&callable` inference.
- Local identities are bounded flow values; capture-free constant callables
  use ordinary file-owned engine facts, replacement, ambiguity, fingerprints,
  project isolation, and persistent dependency products.
- The neutral callable-body suite passes 50/50, including local/cross-file
  navigation, consumer parity, lifecycle replacement, every accepted bound,
  and fail-closed controls.
- `cargo test --workspace` passes: `ruby-analysis` 601/601, root 1,626 passed
  with two intentional ignores, and every packaged extension and doc target
  passes.
- The explicit inference scorecard passes 121/121 at 100/100 with zero gaps
  and zero unexpected outcomes.
- Five alternating neutral release pairs pass every fixed production budget;
  affected edit p95 is +1.79%, retained heap +0.003%, and wall/CPU/RSS improve.
- Five alternating fully warm representative pairs restore 604/604 persistent
  products per run with zero misses, producers, or corruptions. Median CPU is
  +0.10%, retained heap +0.11%, and all RSS samples remain below the fixed
  1,776,846,438-byte ceiling.
- Machine-readable baseline and final evidence live in
  `support/performance/callable-body-inference-baseline-2026-08-12.json` and
  `support/performance/callable-body-inference-final-2026-08-12.json`.

## Objective

Infer the result of statically visible Ruby lambda/proc bodies from their
proven call-site arguments, then reuse that result in direct callable calls and
the existing higher-order call solver.

The first release should make these results deterministic across cold indexing,
open documents, edits, file ordering, and reindexing:

```ruby
stringify = ->(value) { value.to_s }

direct = stringify.call(1)
strings = [1, 2].map(&stringify)

# direct: String
# strings: Array<String>
```

It should also work when a capture-free callable constant is defined in one
file and referenced from another:

```ruby
# converters.rb
module Converters
  STRINGIFY = ->(value) { value.to_s }
end

# report.rb
labels = [1, "ready"].map(&Converters::STRINGIFY)
# labels: Array<String>
```

This must extend the accepted callable-signature architecture. It must not
become a proc-name table, a second type-inference engine, retained Prism nodes,
or a consumer-specific fallback.

## Product Outcome

Users should be able to extract an explicit block into a statically visible
callable without losing type inference, completion, navigation, diagnostics,
or structural precision.

Initial product coverage:

- `->(...) { ... }`, `lambda { ... }`, `proc { ... }`, and
  `Proc.new { ... }` literals whose bodies can be summarized completely.
- Direct local calls through `.call`.
- Passing a proven local or constant callable through `&callable` to an
  existing higher-order signature.
- Capture-free callable constants referenced across files.
- Bounded same-scope local aliases of a proven callable.
- Parameter-dependent method calls, structural Hash reads, array/hash
  construction, local temporaries, and exhaustive branches in callable bodies.
- Proven same-scope captured local reads resolved with Ruby's source-ordered
  binding semantics.
- Strict lambda arity and lenient proc arity, including required, optional,
  and rest parameters within fixed bounds.
- Exhaustive union results and shape-preserving results.

Representative supported cases:

```ruby
project_name = ->(row) { row[:name] }
rows = [{ name: "Ada" }, { name: "Grace" }]
names = rows.map(&project_name)
# Array<String>
```

```ruby
normalize = ->(value) { flag ? value : value.to_s }
results = [1, 2].map(&normalize)
# Array<Integer | String>
```

```ruby
prefix = "item"
format = ->(value) { "#{prefix}:#{value}" }
prefix = :item

format.call(1)
# String: every reachable prefix type still proves String interpolation
```

## Core Soundness Contract

A concrete callable-body result may be published only when all of these are
proven:

1. The callable value has one unique static identity at the use site.
2. Its syntax, arity mode, parameters, body summary, and capture set are
   complete.
3. Every required call argument has a proven type.
4. Lambda/proc arity adaptation is fully represented for the call shape.
5. Every parameter read and supported local temporary has a proven reaching
   value.
6. Every captured binding has one source-ordered proof at the call site.
7. Every reachable body exit has a proven result.
8. Method lookup inside the body is complete for every receiver union member.
9. The callable has not escaped through an unsupported storage or invocation
   boundary.
10. No callable-body, alias, capture, recursion, type-depth, or union bound is
    exceeded.

If any dependency is missing, ambiguous, stale, escaped, unsupported, or out of
bounds, the complete dependent result must be a stable explained `Unknown`.
Never reuse the callable's previous result, drop an unresolved union member,
snapshot a mutable capture at definition time, or widen to `Object`.

Examples:

```ruby
convert = ->(value) { value.to_s }
input = condition ? 1 : unresolved_value
convert.call(input)
# Unknown[incomplete_callable_input]
```

```ruby
convert = ->(value) { value.to_s }
publish(convert) # unsupported escape
loaded = fetch_converter
loaded.call(1)
# Unknown: no static identity connects loaded to convert
```

## Ruby Semantic Rules

### Callable identity

- A direct literal assignment creates one bounded callable identity.
- Source-ordered local aliases may share that identity while the alias bound
  holds.
- Reassigning a local to another callable replaces its identity from that
  source position onward.
- Reassigning it to a non-callable or unresolved value invalidates callable
  proof.
- A constant callable is file-owned. Reopened conflicting definitions are
  ambiguous unless all effective facts are semantically identical.
- A callable stored in an instance/class/global variable, collection, dynamic
  constant target, or unknown call is escaped and cannot later regain identity
  by guesswork.

### Arity

- Lambdas use strict method-like arity.
- Procs use Ruby's lenient positional behavior: missing positional inputs bind
  `NilClass`, and proven extra inputs are ignored only when Ruby would ignore
  them.
- Optional and rest parameters must preserve their exact supported call shape.
- Keyword, destructured, numbered, and forwarding parameters remain Unknown
  until their binding semantics are explicitly modeled.
- The existing four-block-parameter bound also limits callable-body parameters
  in the first release.

### Captures

- A local closure captures a binding, not a frozen type snapshot.
- Read-only same-scope captures resolve from the reaching environment at the
  invocation point.
- An unresolved or ambiguous captured binding makes the dependent result
  Unknown.
- Writes to captured outer locals, mutable captured-object effects, capture
  alias overflow, or invocation after unsupported escape fail closed.
- Cross-file callable constants must be capture-free except for independently
  resolvable constant/method references. A lexical local capture must never be
  serialized as cross-file truth.

### Body flow

- Ordinary fallthrough and supported `next value` exits contribute results.
- `return` is a local callable exit for lambdas but a non-local exit for procs;
  proc `return` remains unsupported.
- Raising paths do not reach the result join.
- Exhaustive branches produce canonical unions.
- `break`, proc non-local `return`, `redo`, `retry`, dynamic throw/catch, and
  unsupported rescue/ensure effects produce an explicit flow-related Unknown.
- Recursive callable invocation remains Unknown until a separately reviewed
  bounded fixed-point model exists.

## Architecture

### One AST-free callable body summary

Add one syntax-independent, compact callable-body domain in
`ruby-analysis::core`. Exact Rust names may change during implementation, but
the domain must represent:

```text
CallableBodyFact (only for file-owned/exportable callable values)
  subject / callable identity
  declaration and body ranges
  arity mode and parameter shape
  capture dependencies
  bounded CallableBodySummary

CallableBodySummary
  parameter reads
  capture reads
  literals and supported construction
  local bindings / source-ordered joins
  method calls and arguments
  structural reads
  exhaustive flow joins
  reachable result exits
```

The summary is a constraint/equation representation, not a second runtime type
algebra. It may refer to parameters and capture dependencies internally, but
only a canonical `RubyType` or stable `UnknownReason` may leave evaluation.

Never retain Prism nodes, reparse a body at each call site, store source slices
as semantic instructions, or execute Ruby.

### Local and cross-file ownership

- `ruby-analysis::indexer` lowers supported callable literals during the
  ordinary scope-aware Prism traversal.
- Local callable summaries live only in the bounded flow environment for that
  file analysis. They do not become a global name map.
- Exportable constant callables enter the engine as ordinary file-owned facts
  through `register_file -> replace_facts -> resolve`.
- Engine state owns deterministic constant-callable identity, ambiguity,
  replacement, memory accounting, and stable fingerprints.
- `AnalysisQuery` exposes a domain query for callable identity/summary; it must
  not expose the backing store.
- Project constant-callable facts are not dependency stubs and must not leak
  into another isolated project engine.
- Edit, delete, parse failure, watcher replacement, and dynamic workspace
  rehoming use the ordinary file lifecycle.

### One evaluator and one higher-order bridge

`ruby-analysis::inference` owns callable instantiation:

1. Resolve one callable identity.
2. Bind call-site arguments using lambda/proc arity rules.
3. Resolve supported captures from the proven environment/query context.
4. Evaluate the compact body constraints through existing type operations and
   `AnalysisQuery` lookup.
5. Join every reachable result or return an explained Unknown.

Direct `.call` and `&callable` must use this same evaluator. For
`&callable`, the existing `PreparedCallableSet` supplies block-input types;
the callable-body evaluator produces the block result; the existing solver
then substitutes the enclosing call result.

The body evaluator may reuse type/shape/call primitives, but must not duplicate
MRO, visibility, overload, ambiguity, or missing-method policy. Those remain
single-sourced in engine queries.

### Consumer boundary

Hover, inlay hints, completion, diagnostics, chained dispatch, navigation, and
`ruby-fast-lsp check` consume the resulting ordinary engine-owned outcome.
No consumer may recognize proc syntax, callable names, or collection methods.

## Fixed Bounds

The first implementation uses these reviewed hard limits:

- four callable parameters;
- 64 callable-body summary nodes;
- eight captured bindings;
- eight live aliases of one callable identity;
- eight nested callable instantiations;
- 16 body-constraint solve iterations;
- eight body-result union variants;
- eight nested structural/type levels, reusing the accepted type/shape depth
  boundary.

Phase 0 must add exact boundary and boundary-plus-one fixtures and record their
cost. A bound may be revised only with updated tests, documentation, and
measurement evidence before implementation depends on it.

Exceeding any bound returns `callable_body_bound_exceeded`. Never truncate a
body, discard captures/aliases, flatten a union, or continue with partial
substitution.

## Stable Unknown Reasons

Extend the reason schema with precise callable-body failures. Exact Rust
variant names may change, but stable external codes must distinguish at least:

- `unsupported_callable_body`;
- `incomplete_callable_input`;
- `incomplete_callable_capture`;
- `ambiguous_callable_value`;
- `escaped_callable_value`;
- `callable_body_bound_exceeded`;
- `callable_recursion_unsupported`; and
- `unsupported_callable_flow`.

Do not collapse these into `unresolved_method_return`, and do not expose
free-form text as the machine-readable contract.

## Phased Plan

### Phase 0 — RED contract, bounds, and baseline

- Add neutral synthetic RED fixtures for parameter-dependent local lambda
  `.call`, `map(&lambda)`, shape projection, exhaustive union results,
  same-scope capture reads, strict/lenient arity, and a cross-file capture-free
  callable constant.
- Add negative controls for an unresolved argument member, unresolved capture,
  non-callable reassignment, escaped callable, ambiguous constant callable,
  proc non-local return, recursion, and every bound.
- Prove the existing parameter-independent callable cases remain GREEN.
- Record release-profile indexing, edit, affected-query, heap, and RSS baseline
  using a clean revision and the accepted measurement workflow.

Exit gate: positives are RED for the intended missing body substitution;
negative controls fail closed or have a recorded bug; exact bounds are reviewed.

### Phase 1 — Callable body domain and lowering

- Add canonical arity, parameter, capture, constraint-node, result-exit, and
  summary types without Prism or LSP dependencies.
- Lower supported callable literals during the ordinary index traversal.
- Represent local temporaries and exhaustive supported flow without retaining
  AST nodes.
- Enforce node, capture, parameter, depth, and union bounds while lowering.
- Add normalization, equality, stable ordering, memory accounting, and
  boundary-plus-one unit tests.

Exit gate: supported bodies lower deterministically; unsupported syntax and
bound excess produce exact reasons; no consumer behavior changes yet.

### Phase 2 — File ownership and callable identity

- Add bounded local callable identities and aliases to the flow environment.
- Add file-owned constant callable facts to engine ingestion/replacement.
- Add deterministic `AnalysisQuery` resolution for unique, identical, missing,
  and ambiguous callable constants.
- Include callable facts in semantic fingerprints, memory estimates, project
  isolation, and any persistent product schema whose semantic producer includes
  them.
- Invalidate identities on non-callable reassignment or unsupported escape.

Exit gate: cold index, edit, parse failure, delete, and cross-file constant
resolution replace facts exactly once with no stale summary or public store API.

### Phase 3 — Direct callable instantiation

- Bind proven `.call` arguments through exact lambda/proc positional arity.
- Evaluate parameter reads, supported local temporaries, literals,
  construction, structural reads, and ordinary method calls.
- Resolve every receiver union member through `AnalysisQuery`.
- Publish one direct-call `TypeInferenceOutcome` and retain callable navigation.
- Support bounded local aliases and capture-free constant callables.

Exit gate: direct local/cross-file calls are GREEN; partial inputs, ambiguity,
escape, and recursion publish stable Unknown reasons.

### Phase 4 — Existing higher-order solver integration

- Replace the parameter-independent `KnownProcType` result shortcut with the
  shared callable-body evaluator.
- Feed `PreparedCallableSet` block inputs into callable parameters.
- Feed the exhaustive callable result back into the existing enclosing-call
  substitution.
- Prove explicit-block, static-symbol, and equivalent callable-body forms agree.
- Remove superseded duplicate proc-body inference only after parity tests pass.

Exit gate: `map(&callable)` and the supported collection pipeline surface are
GREEN without method-name or consumer-specific cases.

### Phase 5 — Captures, flow, shapes, and arity edges

- Resolve same-scope captured reads at invocation using source-ordered binding
  proof rather than definition-time snapshots.
- Support exhaustive branches, fallthrough, raising paths, lambda-local return,
  and supported `next` behavior.
- Preserve canonical shapes and unions through callable bodies.
- Implement strict lambda and lenient proc positional adaptation exactly.
- Fail closed for captured writes/mutable escape, unsupported control flow,
  destructuring/keywords/forwarding, and bound excess.

Exit gate: positive capture/flow/shape/arity cases are precise and every
counterexample remains an explained whole-result Unknown.

### Phase 6 — Lifecycle, determinism, and consumer parity

- Test cold/warm indexing, call-before-definition ordering, repeated seeded file
  orders, open/close, body edit, capture edit, input edit, constant replacement,
  parse failure, watcher deletion, and reindexing.
- Prove delayed producers cannot overwrite newer callable facts or dependent
  outcomes.
- Verify hover, inlay hints, completion, diagnostics, chained dispatch,
  definition/navigation, and check-CLI parity.
- Add scorecard cases and stable reason-schema assertions.

Exit gate: one source revision produces one result on every consumer regardless
of file-open or indexing timing.

### Phase 7 — Documentation, audit, and performance evidence

- Add `docs/callable-body-inference.md` with concise supported/unsupported
  examples and link it from README and the higher-order call guide.
- Update inference Rustdoc, `src/ARCHITECTURE.md`, `AGENTS.md`, `NEXT.md`, and
  the scorecard.
- Run full workspace tests, explicit scorecard reporting, formatting, diff
  checks, and architecture/review audits.
- Run alternating baseline/candidate release measurements on neutral fixtures
  and an anonymized read-only representative multi-project corpus.
- Publish machine-readable evidence under `support/performance/`.

Exit gate: correctness, precision, lifecycle, consumer parity, documentation,
and performance evidence are accepted.

## Acceptance Matrix

| Case | Required result |
| --- | --- |
| `f = ->(x) { x.to_s }; f.call(1)` | `String` |
| `[1, "x"].map(&->(x) { x.to_s })` or equivalent local | `Array<String>` |
| shaped rows projected by a callable | `Array<String>` |
| callable body with exhaustive `Integer`/`String` branches | exact union result |
| same-scope proven capture read | result derived from reaching capture type |
| capture-free callable constant used in another file | same result as local callable |
| strict lambda with incompatible arity | explained Unknown |
| lenient proc with missing positional input | missing parameter bound to `NilClass` |
| one unresolved input union member | whole result Unknown |
| capture reassigned to unresolved value | `incomplete_callable_capture` |
| local callable reassigned to non-callable | no stale callable result |
| callable passed through unknown escape | `escaped_callable_value` when identity is reused |
| conflicting constant callable definitions | `ambiguous_callable_value` |
| recursive callable | `callable_recursion_unsupported` |
| unsupported proc non-local return | `unsupported_callable_flow` |
| summary/capture/alias/depth bound exceeded | `callable_body_bound_exceeded` |
| callable body or constant file edited/deleted | deterministic refreshed result |

## Non-Goals for the First Release

- Executing Ruby, application code, gems, `eval`, or runtime reflection.
- Arbitrary Proc objects returned from unresolved methods.
- `Method` objects, `method(:name).to_proc`, custom `to_proc`, currying,
  composition, partial application, or dynamic callable factories.
- Callables recovered from arrays, hashes, instance/class/global variables,
  serialization, or unknown method calls.
- Cross-file lexical-local captures.
- Writes to captured outer locals or unproven mutation of captured objects.
- Keyword, destructured, numbered, anonymous forwarding, or pattern parameters.
- Recursive callable fixed points.
- Full non-local proc `return`, `break`, `redo`, `retry`, throw/catch, or
  arbitrary ensure semantics.
- Broader multi-site Ruby `yield` flow; that remains a separate follow-up using
  the same callable model.
- Consumer-local fallbacks, refresh loops, indexing-order retries, or guessed
  result reuse.

## Performance Contract

- Fixed production budgets must pass on every candidate run.
- Five alternating baseline/candidate pairs must keep median end-to-end wall
  time, total CPU, affected edit/query p95, and retained engine heap within the
  accepted 3% comparison envelope.
- Warm representative peak RSS must remain below the fixed ceiling documented
  in `AGENTS.md`; all samples and comparison variance must be recorded.
- Files without callable literals must not allocate callable summaries or dense
  per-expression callable state.
- Callable facts and summaries must have exact deep-memory accounting.
- Any separately accepted tradeoff must be explicit in machine-readable
  evidence; do not hide outliers or cache-population runs.

## Completion Criteria

This goal is complete only when:

- Direct calls and `&callable` use one parameter-dependent body evaluator.
- Local and cross-file constant callable identities are deterministic and
  file-owned at their appropriate lifecycle boundary.
- No Prism node, source snippet instruction, parallel semantic store, or
  consumer-specific inference path is retained.
- Lambda/proc arity, captures, exhaustive flow, shapes, ambiguity, escape,
  recursion, and every bound are tested with positive and fail-closed controls.
- Cold/warm indexing, file ordering, edits, parse failures, deletion, and
  reindexing cannot publish stale callable results.
- Hover, hints, completion, diagnostics, navigation, chained dispatch, and CLI
  checks agree on concrete results and Unknown reasons.
- No private project name, path, constant, or copied source appears in tests or
  documentation.
- Full tests, scorecard, architecture review, formatting, performance, memory,
  and machine-readable evidence gates pass.
- `AGENTS.md`, `NEXT.md`, architecture/inference docs, product docs, and the
  scorecard describe the accepted implementation and remaining limits.
