# Structural Hash Shapes

Ruby Fast LSP infers TypeScript-like structural types for Hash-backed Ruby
values when their complete structure is statically proven. The feature is
available in hover, inlay hints, completion, diagnostics, chained method
dispatch, and `ruby-fast-lsp check`.

It does not make Ruby statically typed. Dynamic behavior remains `Unknown`
rather than producing a convenient but unsafe type.

## Quick example

```ruby
payload = {
  id: 42,
  profile: {
    name: "Ada",
    active: true
  }
}

payload
# { id: Integer, profile: { name: String, active: TrueClass } }

payload[:profile][:name]
# String
```

Symbol and String literal keys are supported and remain distinct.

```ruby
payload = { name: "symbol", "name" => "string" }

payload[:name]  # String
payload["name"] # String
```

## Supported behavior

| Area | Supported behavior |
| --- | --- |
| Construction | Exact Symbol/String-keyed Hash literals and nested shapes |
| Flow | Assignments, reachable `if`/ternary branches, ordinary `case`, and supported Hash patterns |
| Correlation | Complete shape variants remain correlated across unions |
| Reads | `[]`, `fetch`, `dig`, `keys`, `values`, `key?`, `has_key?`, `include?`, and `member?` |
| Iteration | Shape evidence flows through `each`, `each_pair`, `each_key`, and `each_value` |
| Mutation | Literal-key `[]=`, `delete`, `clear`, `merge`, `merge!`, and `update` |
| Aliases | Known mutation updates every tracked alias of the same Hash |
| Containment | Nested Hashes and bounded Hashes inside Arrays retain their own identities |
| Propagation | Locals, method returns, parameters with contracts, value constants, and cross-file calls |
| Signatures | Supported RBS records use the same structural shape model |
| Consumers | Hover, inlay hints, key completion, chained dispatch, diagnostics, and CLI checks |

## Branches and unions

Every reachable branch contributes to the result.

```ruby
value = condition ? 42 : "ready"
# Integer | String
```

A missing branch contributes `NilClass`:

```ruby
payload = if condition
  { state: :ready }
end

# NilClass | { state: :ready }
```

A branch that always raises or returns does not reach the join:

```ruby
payload = if condition
  { state: :ready }
else
  raise "failed"
end

# { state: :ready }
```

Shape unions preserve relationships between fields:

```ruby
result = condition \
  ? { kind: :number, value: 42 } \
  : { kind: :text, value: "ready" }

# { kind: :number, value: Integer }
# |
# { kind: :text, value: String }
```

They are not flattened into independent `kind` and `value` unions.

## Discriminated narrowing

Literal equality and inequality narrow correlated variants:

```ruby
if result[:kind] == :number
  result[:value].abs    # Integer
else
  result[:value].upcase # String
end
```

Ordinary `case` and supported Hash patterns use the same proof:

```ruby
case result
in { kind: :number, value: value }
  value.abs    # Integer
in { kind: :text, value: value }
  value.upcase # String
end
```

Narrowing is unavailable after the shape has been invalidated.

## Reads

```ruby
payload = { user: { name: "Ada" }, active: true }

payload[:user]                  # { name: String }
payload[:missing]               # NilClass
payload.fetch(:user)            # { name: String }
payload.fetch(:missing, "none") # String
payload.dig(:user, :name)       # String
payload.keys                    # Array[Symbol]
payload.values                  # Array[TrueClass | { name: String }]
payload.key?(:user)             # TrueClass
payload.key?(:missing)          # FalseClass
```

A dynamic key cannot select one field, so an exact shape returns every
reachable value alternative plus `NilClass`:

```ruby
payload = { name: "Ada", age: 42 }
key = dynamic_key

payload[key] # Integer | NilClass | String
```

## Mutation and aliases

Known literal-key mutation updates the shape:

```ruby
payload = { count: 1 }
payload[:count] = "many"

payload[:count] # String
```

Aliases share one bounded mutable identity:

```ruby
payload = { count: 1 }
copy = payload
copy[:count] = "many"

payload[:count] # String
copy[:count]    # String
```

Known structural mutations are tracked:

```ruby
payload = { id: 1, stale: true }
payload.delete(:stale)
payload.merge!(name: "Ada")

payload # { id: Integer, name: String }
```

`merge` returns a new inferred shape. `merge!` and `update` mutate the receiver.
`clear` produces an empty exact shape. A dynamic key or incomplete merge input
invalidates precision instead of retaining a partial result.

## Cross-file propagation

Method returns propagate without requiring the defining file to be opened:

```ruby
# payload_builder.rb
def build_payload
  { id: 42, name: "Ada" }
end
```

```ruby
# consumer.rb
payload = build_payload
payload[:name] # String
```

Value constants propagate in the same way:

```ruby
# defaults.rb
DEFAULT_USER = { name: "Guest", enabled: true }.freeze
```

```ruby
# consumer.rb
DEFAULT_USER[:enabled] # TrueClass
```

Open editor buffers are authoritative. Editing or deleting the provider
replaces its old shape facts; delayed indexing cannot restore stale types.

## RBS records

Supported RBS records provide structural contracts:

```rbs
class UserService
  def find: (Integer id) -> { id: Integer, name: String }
end
```

```ruby
user = UserService.new.find(42)
user[:id]   # Integer
user[:name] # String
```

Structural diagnostics are emitted only when the complete source and target
types prove incompatibility. Incomplete evidence suppresses the diagnostic.

## Editor behavior

All consumers read the same engine-owned type result:

- Hover and inlay hints use the canonical shape display.
- Chained calls dispatch from the proven selected field type.
- Diagnostics fail closed when any relevant variant is incomplete.
- `ruby-fast-lsp check` matches LSP results.
- Completion offers only literal keys proven on every reachable variant.

```ruby
payload = { id: 42, name: "Ada" }
payload[:|]
# completion: :id, :name
```

String-key completion preserves String syntax. Editing a provider or
invalidating a shape removes stale completion keys.

## Conservative boundaries

### Unknown branch

One unresolved reachable branch makes the whole assignment unknown:

```ruby
payload = condition ? { count: 1 } : dynamic_payload
# Unknown[unresolved_assignment_value]
```

Ruby Fast LSP never keeps only the convenient known branch.

### Unknown Hash splat

```ruby
payload = { known: 1, **dynamic_fields }
# Hash<?, ?>, not a partially trusted { known: Integer, ... }
```

The splat may overwrite a known key, so partial field proof is discarded.

### Escape or unsupported mutation

```ruby
payload = { count: 1 }
dynamic_sink(payload)

payload[:count] # Unknown[mutable_shape_invalidated]
```

An unresolved call may retain or mutate the object. The shape and every known
alias are therefore invalidated. Dynamic dispatch, reflection, `eval`, and
unsupported mutators create the same proof boundary.

### Shallow `freeze`

Ruby's `freeze` is shallow:

```ruby
payload = { profile: { name: "Ada" } }.freeze
payload[:profile][:name] = "Grace" # nested Hash remains mutable
```

The frozen outer Hash retains its key set. Nested mutable values keep separate
mutation and invalidation lifecycles.

### Not arbitrary object shapes

```ruby
class User
  attr_reader :id, :name
end

user = User.new # User, not { id: ..., name: ... }
```

The initial feature models Hash-backed values only. Ordinary Ruby objects keep
nominal class/module lookup.

### No runtime discovery

Ruby Fast LSP does not execute application code or use runtime reflection to
discover payloads. External results need analyzable Ruby code or a supported
YARD/RBS contract before a precise shape can be claimed.

## Fixed limits

Structural inference is deliberately bounded:

| Limit | Maximum |
| --- | ---: |
| Fields per shape | 32 |
| Nested shape levels | 8 |
| Correlated shape variants | 8 |
| Live aliases per mutable identity | 8 |
| Shape solver iterations | 16 |

Exceeding a limit produces `Unknown[shape_bound_exceeded]`. Ruby Fast LSP does
not truncate fields, flatten correlations, or widen the result to `Object`.

## Troubleshooting

If a cross-file shape is missing:

1. Wait for the owning Ruby project's indexing status to become ready.
2. Confirm the file belongs to that project and is not excluded by indexing
   policy.
3. Check whether an unresolved branch, splat, escape, dynamic mutation, or
   bound invalidated the proof.
4. Add an RBS/YARD contract when the implementation is external or dynamic.
5. After installing a development VSIX, run **Developer: Reload Window** so a
   new language-server process uses the packaged binary.

Opening a definition file should not be required after project indexing. If it
changes the final type, that is a lifecycle bug rather than expected behavior.

## Engineering references

- Current architecture: [`src/ARCHITECTURE.md`](../src/ARCHITECTURE.md)
- Inference proof model: [`crates/ruby-analysis/src/inference/mod.rs`](../crates/ruby-analysis/src/inference/mod.rs)
- Reviewed acceptance contract: [`support/type_inference/scorecard.toml`](../support/type_inference/scorecard.toml)
- Accepted limits: [`support/performance/type-inference-shape-bounds-2026-08-11.json`](../support/performance/type-inference-shape-bounds-2026-08-11.json)
- Release evidence: [`support/performance/type-inference-shapes-final-2026-08-12.json`](../support/performance/type-inference-shapes-final-2026-08-12.json)
