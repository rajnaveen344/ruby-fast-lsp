# Callable-Body Inference

Ruby Fast LSP can instantiate a statically visible lambda or proc body from
proven call-site inputs. Direct `.call` and higher-order `&callable` use the
same bounded, parser-free body summary.

## Supported

```ruby
stringify = ->(value) { value.to_s }

direct = stringify.call(1)       # String
mapped = [1, 2].map(&stringify)  # Array<String>
```

Supported bodies include parameter and read-only same-scope capture reads,
local temporaries, literals, arrays, exact Hash shapes, proven method calls,
shape reads, and exhaustive `if`/`unless` flow. Ordinary `case` flow preserves
callable identity at the use site. Lambdas have strict positional arity; procs
have Ruby's lenient required/optional/rest behavior.

A capture-free constant remains usable across files:

```ruby
# converters.rb
module Converters
  STRINGIFY = ->(value) { value.to_s }
end

# report.rb
labels = [1, "ready"].map(&Converters::STRINGIFY)
# Array<String>
```

Cross-file facts use ordinary file ownership. Edits, parse failures, deletion,
reindexing, and conflicting reopened definitions replace or invalidate the
same evidence; opening the defining file is not required.

## Fail-closed boundaries

The complete result is an explained `Unknown` when a required argument or
capture is incomplete, callable identity is ambiguous, the value escaped
through unsupported storage/invocation, flow is unsupported, recursion is
detected, method resolution is incomplete, or a bound is exceeded. No prior
result, partial union, or known field prefix survives that proof failure.

Unsupported first-release forms include keyword, destructured, numbered, and
forwarding parameters; writes to captured outer locals; proc non-local
`return`; rescue/ensure effects; and arbitrary runtime `Proc` objects.

Fixed bounds are four parameters, 64 summary nodes, eight captures, eight live
aliases, eight nested callable instantiations, 16 call-constraint solve steps,
eight result-union variants, and eight structural/type levels. Boundary +1
returns `Unknown[callable_body_bound_exceeded]`.

## Architecture

The indexer lowers the already parsed Prism tree once into a compact
`CallableBodySummary`; it retains no AST node or source instruction. Local
identities stay in bounded flow state. Capture-free constants become ordinary
file-owned inference facts, resolve through `AnalysisQuery`, and participate in
semantic fingerprints and persistent dependency products.

One inference evaluator binds Ruby arity, resolves captures, applies canonical
shape/type operations, and delegates method lookup to the engine. Hover, inlay
hints, completion, diagnostics, chained dispatch, navigation, and
`ruby-fast-lsp check` consume its ordinary `TypeInferenceOutcome`; consumers do
not recognize lambda syntax or collection method names.
