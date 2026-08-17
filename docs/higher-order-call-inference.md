# Higher-Order Call Inference

Ruby Fast LSP can infer a call whose result depends on a block when one
complete callable signature proves the receiver, arguments, block inputs,
block result, and substituted call result.

## Supported

```ruby
strings = [1, 2].map { |value| value.to_s }
# Array<String>

strings = [1, "two"].map(&:to_s)
# Array<String>

rows = [{ name: "Ada" }, { name: "Grace" }]
names = rows.map { |row| row[:name] }
# Array<String>
```

Core RBS signatures drive `map`/`collect`, `filter_map`,
`select`/`filter`/`reject`, `each`, and `each_with_object`. Project RBS block
signatures use the same solver:

```rbs
class Transformer
  def apply: [Input, Output]
    (Input value) { (Input) -> Output } -> Output
end
```

The first release also recognizes these bounded Ruby forms:

```ruby
def transform(value)
  yield(value)
end

def transform_all(values, &block)
  values.map(&block)
end

convert = ->(value) { value.to_s }
[1, 2].map(&convert)
# Array<String>
```

Parameter-dependent lambda/proc bodies use the same higher-order substitution
path when their body has a complete bounded proof. See
[Callable-Body Inference](callable-body-inference.md) for body syntax,
captures, cross-file constants, identity, and bounds.

## Flow behavior

All reachable ordinary block results and `next value` exits form one canonical
union. Implicit fallthrough contributes `NilClass`; raising paths do not reach
the join.

```ruby
values.map { |value| flag ? value : value.to_s }
# Array<Integer | String>
```

`filter_map` removes only proven `NilClass` and `FalseClass` alternatives.
`each_with_object` returns a changed structural accumulator only while the
existing bounded mutable-Hash identity proves the mutation.

## Fail-closed boundaries

The complete dependent result is `Unknown` when any required input is missing
or ambiguous. This includes:

- an unresolved receiver or union member;
- incompatible or conflicting callable overloads;
- a dynamic `&callable` without a known signature;
- a static `&:method` unresolved on any input member;
- unsupported destructuring or forwarding;
- `break`, non-local `return`, `redo`, or `retry` in the block;
- dynamic accumulator keys, escape, or invalidated aliases; and
- an exceeded fixed solver bound.

The fixed limits are eight compatible overloads, eight type variables, four
block parameters, 16 binding iterations, eight template levels, and eight
block-result union variants. A limit is never handled by truncating or widening
to `Object`.

## Architecture and lifecycle

RBS declarations and bounded Ruby yield/forwarding relations become ordinary
file-owned method facts. `ruby-analysis::inference` selects signatures and
solves substitutions through `AnalysisQuery`. Only a canonical `RubyType` or a
stable explained `Unknown` leaves that proof boundary.

Hover, inlay hints, completion, chained dispatch, diagnostics, navigation, and
`ruby-fast-lsp check` consume the same engine-owned result. Edits, parse
failures, watcher deletion, close/reopen, cold indexing, and reindexing replace
the same file-owned evidence; no consumer cache or indexing-order retry is part
of inference.
