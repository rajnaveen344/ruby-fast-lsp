# Goal: Proof-First Structural Shapes for Ruby

Status: **Complete — Phases 0–8 accepted**

## Objective

Add TypeScript-style structural shape inference for Ruby values without
pretending that dynamic or escaped mutable state is statically known.

The initial feature models Hash-backed structural values such as API payloads,
configuration objects, options, and service results:

```ruby
def build_user
  {
    id: 42,
    profile: {
      name: "Ada",
      active: true
    }
  }
end

user = build_user
# user: { id: Integer, profile: { name: String, active: TrueClass } }
```

Shapes must propagate through local flow, constants, method returns,
cross-file calls, arrays, unions, and supported RBS records. Hover, inlay
hints, completion, diagnostics, and `ruby-fast-lsp check` must project the same
engine-owned result.

This is not a promise to infer every Ruby Hash. When mutation, aliasing,
reflection, or an unresolved call makes the structure uncertain, inference
must widen or return an explained `Unknown` rather than retain a convenient
stale shape.

## Why This Is Feasible in Ruby

Ruby's dynamic typing does not prevent static reasoning about visible control
flow. An unknown condition means every reachable branch contributes to the
result:

```ruby
if condition
  value = 1
else
  value = "ready"
end

# value: Integer | String
```

At each program point, inference tracks the type proven on that path. At a
join, it forms the exhaustive union of the reachable path types. A branch that
always raises or returns does not reach the join. A missing branch contributes
the previous value or `NilClass`, as Ruby semantics require.

The same rule extends to shapes, but correlations between fields must be
preserved:

```ruby
result =
  if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
```

The semantic result is a union of complete variants:

```text
{ kind: :number, value: Integer }
|
{ kind: :text, value: String }
```

It must not be flattened to the following type:

```text
{ kind: :number | :text, value: Integer | String }
```

Flattening loses the relationship between `kind` and `value` and makes
discriminated narrowing impossible.

## Soundness Contract

The feature is proof-first relative to the indexed Ruby program and declared
contracts.

1. Every reachable branch participates in a join.
2. Diverging branches do not contribute a result at the join.
3. One unsupported or unknown variant prevents a partial concrete claim where
   completeness is required.
4. Shape variants remain correlated; field-wise display compression must not
   alter semantic identity.
5. Known mutations update the current abstract shape.
6. Mutation through a tracked alias updates every alias of that abstract Hash.
7. An unresolved mutation or escape invalidates every affected mutable alias.
8. Frozen outer Hashes retain their key set, while nested mutable values keep
   their own independent lifecycle.
9. Reflection, unconstrained `send`, `eval`, native behavior without a
   declaration, and unbounded metaprogramming remain dynamic boundaries.
10. Exceeding a fixed shape, union, depth, alias, or solver bound produces an
    explained `Unknown`; it never silently drops fields or widens to `Object`.

This should be stricter than TypeScript in places. TypeScript deliberately
accepts selected unsound JavaScript patterns. Ruby Fast LSP must retain its
existing rule that incomplete evidence cannot prove a concrete result or a
diagnostic.

## Proposed Type Model

Add domain types in `ruby-analysis::core`; storage IDs remain private to the
engine.

```text
RubyType::Literal(...)
RubyType::Shape(ShapeType)

ShapeType
  fields: canonical bounded list of ShapeField
  rest: optional generic key/value contract
  exactness: exact or open
  stability: frozen or tracked-mutable proof state

ShapeField
  key: LiteralKey
  value: RubyType
  presence: required or optional
```

Initial literal keys:

- Symbols
- Strings

Additional literal key forms require a reviewed use case. Dynamic keys project
through the generic Hash key/value view and cannot select one exact field.

`RubyType::Shape` remains a Ruby `Hash` instance for ordinary method lookup.
Inference exposes canonical key and value projections when an RBS `Hash[K, V]`
method is selected. Shape-specific operations may retain more precision than
that generic projection.

## Control-Flow Semantics

### Primitive branch assignment

```ruby
if condition
  value = 1
else
  value = "ready"
end
```

- True branch: `Integer`
- False branch: `String`
- Join: `Integer | String`

Only methods valid for every reachable union member can produce a proven
chained result without narrowing.

### Missing branch

```ruby
if condition
  value = 1
end
```

If `value` had no prior assignment, the post-join type is
`Integer | NilClass`. If it had a prior value, that prior type participates in
the join.

### Diverging branch

```ruby
if condition
  value = 1
else
  raise "failed"
end
```

The post-join type is `Integer` because the other branch cannot reach it.

### Shape branches

Shape branches remain a union of variants. Structurally identical variants may
be deduplicated. They must not be field-wise merged when doing so would lose
cross-field correlation.

### Discriminated narrowing

Literal field checks narrow a shape union:

```ruby
if result[:kind] == :number
  result[:value].abs
else
  result[:value].upcase
end
```

The true path retains only variants whose required `:kind` field can equal
`:number`. The false path removes variants whose required field is exactly
`:number`. Optional fields, generic rest keys, unknown values, and mutable
escaped shapes narrow only when the proof remains exhaustive.

## Structural Compatibility

Shape compatibility is directional:

- Every required target field must exist in the source with a compatible
  value type.
- An optional target field may be absent.
- Extra source fields satisfy an open target shape.
- An exact target rejects unaccounted source fields.
- A source rest contract must be compatible with every target field it may
  supply.
- `Unknown` is not a structural wildcard.
- Generic `Hash<K, V>` is not assumed to satisfy a required structural field.

RBS record types should enter this same model rather than degrading to
`Hash<?, ?>`. RBS interfaces and arbitrary object method-shapes are a separate
future decision; the first project is specifically Hash-backed structural
values.

## Mutation, Aliasing, and Escape

Mutation is the main correctness risk.

### Known mutation

```ruby
payload = { count: 1 }
payload[:count] = "many"
```

The current shape becomes `{ count: String }` after the write.

### Known alias

```ruby
payload = { count: 1 }
copy = payload
copy[:count] = "many"
```

`payload` and `copy` must share one bounded abstract Hash identity, so both
observe `{ count: String }` after the write.

### Deletion and clearing

- `delete(:known_key)` removes that required field from the reaching shape.
- A dynamic delete makes matching fields optional or invalidates the exact
  shape when the affected set cannot be bounded.
- `clear` produces an exact empty shape for a tracked mutable Hash.

### Merge and splat

- A literal `**shape` or `merge` composes fields in Ruby overwrite order.
- Unknown splats add or widen a rest contract when that is provable.
- Otherwise the result becomes explained `Unknown` or a generic Hash; known
  fields must not survive as a misleading partial shape.

### Escape

Passing a mutable shape to an unresolved call, storing it through an
untracked/global boundary, or invoking an unsupported mutator invalidates the
shape proof for all tracked aliases after that point. A later phase may
preserve shapes across methods with an explicit non-mutating contract, but the
initial implementation must fail closed.

## Supported Operations by Milestone

Initial precise operations:

- Hash literal construction
- `[]`
- `fetch`
- Nested `dig`
- `key?` / `has_key?`
- Literal `[]=`
- `delete`
- `clear`
- Literal `merge` / `merge!`
- Hash splat (`**`)
- `keys`
- `values`
- Hash pattern matching

Other ordinary Hash methods continue through the existing RBS method lookup
using the shape's generic key/value projection. Unsupported mutators invalidate
mutable shape precision.

## Architecture and Ownership

The existing one-way semantic path remains unchanged:

```text
Prism traversal in ruby-analysis::indexer
        -> file-owned facts, flow evidence, and constraints
        -> engine graph and immutable query context
        -> bounded shape inference, joins, narrowing, and substitution
        -> engine-owned solved outcomes
        -> thin LSP and check-CLI projections
```

Layer ownership:

- `ruby-analysis::core`: literal keys, shape domain types, canonical limits,
  and public semantic values.
- `ruby-analysis::indexer`: recognize Hash literals, literal keyed operations,
  mutations, aliases, and escape syntax during the existing traversal.
- `ruby-analysis::inference`: construct shapes, join control-flow variants,
  track bounded abstract Hash identities, narrow discriminated unions, apply
  mutation, and convert RBS records.
- `ruby-analysis::engine`: store compact file-owned outcomes and expose
  deterministic domain queries. Existing engine method/MRO/visibility policy
  remains the only lookup authority.
- `src/*`: convert domain results to hover, inlay, completion, diagnostics, and
  CLI output. No adapter may infer or merge shapes independently.

No parallel shape store, request-time reparse, LSP-only inference path, or
framework-specific Hash rule is allowed.

## Implementation Plan

Implementation starts only after this document is reviewed and approved.

### Phase 0: Acceptance contract and RED evidence

1. Add neutral synthetic scorecard cases for primitive joins, nested shapes,
   shape unions, missing branches, diverging branches, and discriminated
   narrowing.
2. Add negative cases for dynamic keys, unknown splats, alias mutation, escape,
   excessive width/depth, and incomplete union evidence.
3. Record the current expected failures before adding a shape type.
4. Define fixed initial limits for fields, depth, union variants, aliases, and
   solve iterations from small measurements rather than arbitrary growth.

Gate: the positive cases fail for the documented reason, while existing
generic Hash behavior and safety cases remain green.

### Phase 1: Canonical shape and literal type algebra

1. Add bounded literal Symbol/String types.
2. Add canonical required/optional shape fields, exact/open shape state, and a
   generic rest contract.
3. Implement normalization, equality, ordering, hashing, display,
   substitution, containment, generic Hash projection, and deep memory
   accounting.
4. Keep structurally distinct union variants separate.
5. Add exhaustive match coverage for every `RubyType` consumer.

Gate: domain tests pass with no LSP changes and fixed memory-layout/deep-weight
coverage is updated.

### Phase 2: Local literal construction and flow propagation

1. Construct shapes from static Hash literals and nested literals.
2. Propagate them through local assignment and `if`/`unless`/`case` joins.
3. Preserve prior-value and implicit-nil paths correctly.
4. Exclude diverging branches.
5. Support literal Hash splats with Ruby overwrite order.

Gate: local hover/check-domain tests prove shapes and variant unions, but no
mutation-sensitive lookup is published until Phase 3 invalidation is present.

### Phase 3: Mutable identity, aliasing, and invalidation

1. Introduce bounded flow-local abstract Hash identities.
2. Make local aliases share identity.
3. Apply known writes, delete, clear, merge, and merge! to every live alias.
4. Detect escape and unsupported mutation boundaries.
5. Invalidate or widen all affected aliases after an uncertain boundary.
6. Preserve exact outer keys for frozen Hashes without claiming deep freeze.

Gate: no stale field proof survives a known alias mutation, unresolved escape,
or edit/reindex lifecycle.

### Phase 4: Keyed reads and generic Hash behavior

1. Implement exact literal `[]` and `fetch` results.
2. Implement nested `dig` through proven required/optional fields.
3. Implement `key?`/`has_key?` field-presence narrowing.
4. Project `keys`, `values`, `each`, and ordinary RBS Hash calls through
   canonical key/value unions.
5. Keep dynamic-key reads conservative and include `NilClass` where Ruby can
   miss.

Gate: every result is correct for required, optional, absent, rest, dynamic,
and invalidated shapes.

### Phase 5: Discriminated shape unions

1. Narrow union variants using literal equality and inequality on required
   fields.
2. Extend narrowing to supported `case` and Hash pattern forms.
3. Preserve exhaustive else/unmatched paths.
4. Reject narrowing when optional, rest, mutation, or Unknown evidence makes
   the discriminator inconclusive.

Gate: correlated `kind`/`value` examples resolve correctly, and incomplete
discriminators remain Unknown.

### Phase 6: Contracts and cross-file propagation

1. Convert supported RBS records to the canonical shape model.
2. Propagate shapes through method-return equations, value constants,
   parameters with contracts, and cross-file calls.
3. Define structural compatibility for diagnostics only when both sides are
   complete.
4. Preserve provenance and ordinary per-file replacement semantics.

Gate: cold index, early-open, edit, and stale-fact removal produce identical
final results independent of file and batch order.

### Phase 7: Consumer parity and editor UX

1. Render one canonical shape form in hover, inlay hints, and the check CLI.
2. Use the same engine outcome for completion and chained dispatch.
3. Add literal key completion only after semantic key availability is proven.
4. Keep diagnostics fail-closed for incomplete structural compatibility.

Gate: differential CLI/LSP tests prove parity; adapters contain formatting and
position conversion only.

### Phase 8: Performance and release acceptance

1. Run the complete scorecard and reviewed real-project precision suite.
2. Measure cold/warm indexing, typing latency, query latency, CPU, allocation,
   and peak RSS in release mode.
3. Measure retained shape counts, average/max fields, union widths, alias-set
   sizes, invalidations, and bound-triggered Unknowns.
4. Reject or redesign any representation that breaches fixed performance or
   memory gates.
5. Update `NEXT.md`, `src/ARCHITECTURE.md`, inference Rustdoc, and `AGENTS.md`
   only after the accepted implementation establishes current truth.

## Acceptance Examples

The reviewed feature is not complete until all of these categories pass.

### Exact nested shape

```ruby
payload = { user: { name: "Ada", age: 42 } }
name = payload[:user][:name] # String
```

### Primitive branch union

```ruby
value = condition ? 1 : "ready"
# Integer | String
```

### Correlated shape union

```ruby
result = condition \
  ? { kind: :number, value: 1 } \
  : { kind: :text, value: "ready" }
```

### Discriminated narrowing

```ruby
if result[:kind] == :number
  result[:value].abs
else
  result[:value].upcase
end
```

### Alias mutation

```ruby
payload = { count: 1 }
copy = payload
copy[:count] = "many"
value = payload[:count] # String, never stale Integer
```

### Unknown escape

```ruby
payload = { count: 1 }
unknown_call(payload)
value = payload[:count] # no stale concrete claim
```

### Cross-file method return

```ruby
# builder.rb
def build_payload
  { id: 1, name: "Ada" }
end

# consumer.rb
payload = build_payload
name = payload[:name] # String
```

### Edit invalidation

Changing `name: "Ada"` to `name: dynamic_call` must remove the prior `String`
proof from every local and cross-file consumer. A delayed cold-index result
must not restore the stale shape.

## Non-Goals for the First Project

- Full TypeScript syntax or annotations in Ruby source
- TypeScript's mapped, conditional, `keyof`, template-literal, or utility types
- Structural typing of arbitrary Ruby objects by their method sets
- Treating every class instance variable set as a structural object shape
- Runtime execution or reflection to discover Hash contents
- Assuming unknown calls are non-mutating
- Deep immutability from Ruby's shallow `freeze`
- Unbounded recursive or self-referential shapes
- Framework-specific payload schemas in the core analyzer
- Replacing nominal Ruby class/module method lookup with structural dispatch

## Approved Review Decisions

1. Phase 1 targets Hash-backed shapes only; arbitrary object method-shapes
   remain out of scope.
2. Initial literal keys are Symbol and String only.
3. Inferred literal shapes are exact. Structural contracts are open unless an
   explicit exact contract is available.
4. Unresolved calls invalidate mutable shape precision after the call.
5. Semantic unions preserve complete variants even if a future consumer offers
   a correlation-safe compact display.
6. RBS record support remains in the first release after local shapes and
   mutation safety are proven.
7. The initial fixed bounds are 32 fields per shape, eight nested shape levels,
   eight shape variants per union, eight live aliases per mutable identity, and
   16 shape-solver iterations. The aggregate evidence and selection rationale
   are recorded in
   `support/performance/type-inference-shape-bounds-2026-08-11.json`.

Phase progress: Phase 0 fixed the neutral RED acceptance contract and measured
bounds; Phase 1 established the canonical literal/shape algebra; Phase 2 proved
local construction and control-flow propagation; Phase 3 established bounded
flow-local identities, alias-wide known mutation, escape invalidation, shallow
freeze semantics, and edit/reindex replacement; Phase 4 established exact and
dynamic keyed reads, fetch/dig, presence narrowing, generic Hash projections,
top-level flow parity, and explained shape-bound failures; Phase 5 established
equality/inequality, ordinary `case`, and supported Hash-pattern narrowing while
retaining optional, rest, invalidated, and otherwise inconclusive variants on
every reachable path. Phase 6 established canonical RBS record conversion,
exhaustive compatible-overload contracts, cross-file method-return and value-
constant propagation, parameter flow, complete-only structural diagnostics,
nested mutable identity through Hash and bounded Array containment, and
deterministic early-open, reverse-order, edit, watcher replacement, and stale-
fact removal behavior. Phase 7 established canonical hover, inlay, and CLI
rendering; exact engine-owned completion and chained-dispatch proofs; proven
Symbol/String literal-key completion with UTF-16-correct edits; fail-closed
structural diagnostics; and differential CLI/LSP lifecycle evidence. Its edit
gate also proves that a newly invalidated local method return blocks fallback
to the previous engine snapshot while the replacement file is being derived.
Phase 8 passed the complete 111-case scorecard at 100/100 with no recorded
gaps or unexpected outcomes, the 13-case reviewed precision corpus with zero
known false positives, the serialized workspace test suite, the fixed release
latency and memory budgets, two deterministic warm representative-corpus runs,
and an exact-source DHAT allocation audit. The accepted measurements and build
identities are recorded in
`support/performance/type-inference-shapes-final-2026-08-12.json`. `NEXT.md`,
`src/ARCHITECTURE.md`, inference Rustdoc, and `AGENTS.md` now describe the
implemented architecture and its proof-first boundaries.
