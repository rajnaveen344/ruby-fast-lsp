# Next Engineering Goals

Ruby Fast LSP now has a proof-first type-inference foundation shared by the
language server and `ruby-fast-lsp check`. The next phase should improve the
number of statically provable Ruby programs without weakening determinism,
diagnostic precision, edit latency, or memory bounds.

Hash-backed structural shapes are current functionality, not roadmap work.
They preserve correlated union variants, track bounded mutable aliases,
invalidate on unsupported mutation or escape, interoperate with supported RBS
records, and project one engine-owned result to editor and CLI consumers. New
roadmap slices may build on that algebra but must not introduce a parallel
object-shape store or treat `Unknown` as a structural wildcard.
The concise behavior contract and examples live in
[`docs/structural-hash-shapes.md`](docs/structural-hash-shapes.md).

This is a forward-looking roadmap, not an implementation log or release
checklist. Current architecture belongs in `src/ARCHITECTURE.md` and the
inference module Rustdoc. Current acceptance results belong in
`support/type_inference/scorecard.toml`; measurements belong under
`support/performance/`.

Higher-order calls and generic block inference are current functionality, not
roadmap work. One bounded, signature-driven callable model relates receiver and
method generics, block inputs and exhaustive results, and the call result for
explicit blocks, core collection transforms, static `&:method`, statically
known callable values, bounded forwarding/direct-yield forms, and project RBS.
The concise behavior contract lives in
[`docs/higher-order-call-inference.md`](docs/higher-order-call-inference.md).

Parameter-dependent callable bodies are also current functionality. Statically
visible lambda/proc literals are lowered once into bounded AST-free summaries;
direct `.call` and `&callable` share one evaluator, while capture-free constant
callables retain ordinary cross-file ownership and persistence. The concise
contract lives in
[`docs/callable-body-inference.md`](docs/callable-body-inference.md).

## P0: Broader Ruby yield flow

Build on the accepted callable model without adding a second inference path.
The next precision slice is broader Ruby-defined yielding methods whose yield
sites require ordinary control-flow joins rather than the current bounded
direct relation.

Representative next outcome:

```ruby
def transform(values)
  return yield(values.first) if fast_path?
  yield(values.last)
end
```

Exit criteria:

- Multiple compatible direct yield sites join through ordinary flow evidence.
- Explicit block, static-symbol, and known-callable forms remain equivalent
  when they have the same complete proof.
- Scorecard, precision, lifecycle, and release performance gates pass.

## P1: Type algebra needed by callable solving

Add type forms only when a reviewed inference case requires them. The likely
next forms are:

- callable/block/proc signatures;
- explicit type variables and generic applications beyond the current
  `Array`/`Hash` representation;
- `Never`/bottom for non-returning expressions and reachable joins;
- `Untyped` as an explicit gradual escape hatch distinct from missing proof;
  and
- supported RBS tuples, intersections, self types, and aliases beyond the
  current record conversion.

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
