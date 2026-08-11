# Next Engineering Goals

Ruby Fast LSP now has a proof-first type-inference foundation shared by the
language server and `ruby-fast-lsp check`. The next phase should improve the
number of statically provable Ruby programs without weakening determinism,
diagnostic precision, edit latency, or memory bounds.

This is a forward-looking roadmap, not an implementation log or release
checklist. Current architecture belongs in `src/ARCHITECTURE.md` and the
inference module Rustdoc. Current acceptance results belong in
`support/type_inference/scorecard.toml`; measurements belong under
`support/performance/`.

## P0: Higher-order calls and generic block inference

Build one reusable model for methods whose result depends on a block or proc.
The current analysis supports selected yielding methods and proc/lambda paths,
but it does not have a general callable constraint model that relates:

- receiver type arguments;
- method type variables and overloads;
- block parameter types;
- block return types; and
- the resulting method type.

This layer should support explicit blocks, lambdas/procs, block forwarding, and
static symbol-to-proc forms such as `&:to_s`. It should be driven by Ruby/RBS
signatures rather than a hard-coded list of collection method names.

Representative outcome:

```ruby
values = [1, 2, "1", "2"]
strings = values.map(&:to_s) # Array<String>
strings.first.upcase         # String receiver
```

The proof is valid only if the element type is complete, `to_s` resolves for
every reachable element member, every selected return is proven, and generic
substitution produces one canonical result. One unresolved member, callable,
overload, or substitution keeps the result Unknown.

Exit criteria:

- `map`, `collect`, `filter_map`, `each_with_object`, and representative
  user-defined/RBS yielding methods use the same abstraction.
- Static `&:method` and its equivalent explicit block produce the same type.
- Union receivers are exhaustive and partial evidence remains Unknown.
- Hover, inlay hints, completion, chained dispatch, navigation, diagnostics,
  and the check CLI consume the same stored outcome.
- Edit/reindex tests prove stale block and result types are removed.
- Scorecard, real-project precision, and release performance gates pass.

## P1: Type algebra needed by callable solving

Add type forms only when a reviewed inference case requires them. The likely
next forms are:

- callable/block/proc signatures;
- explicit type variables and generic applications beyond the current
  `Array`/`Hash` representation;
- `Never`/bottom for non-returning expressions and reachable joins;
- `Untyped` as an explicit gradual escape hatch distinct from missing proof;
  and
- supported RBS tuples, records, intersections, self types, and aliases.

`Unknown`, `Untyped`, and `Never` must remain semantically distinct. Unknown
withholds a claim because evidence is insufficient. Untyped permits an explicit
dynamic boundary without pretending a concrete type was inferred. Never is
private or public reachability evidence that disappears from reachable joins.

All composite construction must be canonical and bounded. Exceeding union,
type-depth, or solver limits returns an explained Unknown; it must not widen to
`Object` or retain a convenient partial answer.

Exit criteria:

- New forms have normalization, equality, display, substitution, containment,
  and memory-accounting tests.
- The engine stores compact deterministic representations without exposing
  storage IDs as semantic identity.
- Existing concrete labels and CLI/LSP parity remain unchanged unless a
  reviewed case deliberately becomes more precise.
- Retained-state and hot-path changes pass the fixed performance/RSS contract.

## P1: One reusable binding and constraint seam

Reduce duplicated inference rules incrementally across `TypeTracker`,
`FactCollector`, RBS helpers, completion probing, and query fallbacks. Do not
replace the working engine wholesale.

The target seam is compact file-owned binding/constraint data emitted during
the existing scope-aware Prism traversal. It should describe definitions,
uses, calls, joins, callable inputs/results, and dependencies without retaining
Prism nodes. Inference solves that data against an immutable engine query
context; solved outcomes re-enter the ordinary file replacement lifecycle.

Exit criteria:

- One ordinary source parse and one primary scope traversal remain sufficient.
- Equivalent feature-local rules are removed only after differential parity
  tests prove the shared rule.
- Engine method/MRO/visibility/ambiguity resolution remains the sole lookup
  policy.
- Recursive and cross-file dependencies remain bounded and schedule
  independent.
- No parallel semantic store or LSP-specific inference API is introduced.

## P1: Complete explanations for false Unknowns

Every supported inference site that withholds a concrete type should expose a
stable reason. Extend the current reason codes and bounded evidence only where
it helps distinguish actionable coverage gaps, for example:

- unsupported callable or block form;
- incomplete generic substitution;
- ambiguous overload;
- incomplete dependency or ancestor chain;
- invalidated refinement; and
- solver width, depth, or iteration bound.

Explanations are diagnostic and tooling data, not permission to guess. They
must be deterministic, bounded in retained memory and output size, replaced
with their owning file, and shared by CLI and editor projections.

Exit criteria:

- The scorecard can classify false Unknowns by stable reason rather than text.
- The CLI and hover expose equivalent normalized reasons at parity sites.
- Explanation collection does not add dense per-expression storage to files
  that do not need it.

## P2: Precise inferred-export invalidation

Use explicit semantic dependencies to narrow background recomputation when an
inferred public return, constant, attribute, superclass, mixin, signature, or
extension fact changes. Body-only edits must continue to refresh only the
active/open projection and must never synchronously check closed dependents.

Prefer file/module-level invalidation until profiles prove that finer-grained
bindings materially improve real workloads. Cache identity must use exact
content and semantic-export inputs, not timestamps or discovery order.

Exit criteria:

- Body-only changes schedule no closed-file dependent inference.
- Export changes recompute only proven dependents outside the typing-critical
  path.
- Cancellation cannot publish an older solve into a newer document or project
  generation.
- Multi-root engines remain isolated and shared immutable products never carry
  project-specific solved state.
- Cold, warm, edit, query, CPU, and peak-RSS comparisons pass.

## Ongoing coverage work

After the infrastructure above, reduce false Unknowns in priority order from
reviewed scorecard and real-project evidence. Likely areas include richer block
and forwarding shapes, additional RBS forms, safe narrowing, attributes and
generated APIs, and common Ruby control-flow expressions.

Framework-specific knowledge stays in validated extensions. Arbitrary string
evaluation, data-dependent reflective dispatch, unconstrained
`method_missing`, native behavior without declarations, and unbounded runtime
metaprogramming remain intentional dynamic boundaries.

## Rules for every roadmap slice

1. Start with the smallest failing positive case and prove the intended
   failure before implementation.
2. Add a negative or partial-evidence case that must remain Unknown.
3. Implement the rule in the owning reusable layer; adapters only project it.
4. Test all affected consumers and stale-fact removal after edits.
5. Update the reviewed scorecard or precision corpus without reweighting it to
   improve the score.
6. Record proportionate release-build evidence for material CPU, latency,
   allocation, or retained-memory changes.
7. Reject a faster or more permissive design if it weakens proof completeness,
   determinism, project isolation, or a fixed performance gate.
