# FactCollector

`FactCollector` traverses one Ruby file and collects body types, declarations,
reference and diagnostic candidates, extension contributions, and local scopes.
The engine owns persistent semantic state and cross-file resolution.

## Start here

1. `mod.rs` defines the ten private state owners and constructs a file pass.
2. `traversal.rs` implements Prism's `Visit` trait. Read it to understand entry,
   child traversal, exit, and final method-return solving order.
3. `nodes/` groups syntax handlers into calls, constants, declarations, and
   variables. Shared block and parameter handling stays directly in `nodes/`.
   Ruby block execution-context rules live in `nodes/block_node.rs`.
4. `collection/facts.rs` packages output through `FactCollector::finish()`.

The visitor still follows the same recursive AST traversal. Splitting its
implementation across modules does not introduce additional parsing passes.

## Folder map

```text
fact_collector/
  mod.rs                 Collector construction and private state owners
  traversal.rs           Prism visit order
  tests.rs               Traversal and evidence regressions
  README.md              This reading guide
  context/               Options, extension context, source, lookup inputs
  collection/            Declaration recording and completed file output
  inference/             Expressions, constants, flow, callables, returns
  nodes/
    mod.rs               Node-family declarations
    block_node.rs        Shared block scopes and execution contexts
    parameters_node.rs   Shared parameter collection
    calls/               Calls, super calls, and call diagnostics
    constants/           Constant reads, paths, and writes
    declarations/        Classes, modules, methods, and aliases
    variables/           Local and nonlocal reads and writes
```

Every folder has at most ten immediate entries, including module files and
subfolders. No folder-size exception is needed here. The node families preserve
the correspondence with Prism node types while making related handlers easy to
find; block and parameter handlers are shared across calls and declarations.

## State ownership

| Collector field | Owns | Implementation |
| --- | --- | --- |
| `document` | Source coordinates and collected local variable scopes | `RubyDocument` |
| `scope_tracker` | Active lexical namespace, method, visibility, and execution context | `ScopeTracker` |
| `options` | Body-inference and diagnostic collection choices | `context/options.rs` |
| `extensions` | Host, project context, enclosing calls, and pending block context | `context/extensions.rs` |
| `facts` | Collected declarations, local type staging, candidates, diagnostics, and extension output | `collection/facts.rs` |
| `flow` | Block parameters, pattern captures, multi-assignment elements, callable aliases, yields, and active writes | `inference/flow.rs` |
| `semantics` | The owning engine, per-pass query cache, and same-pass lookup inputs | `context/semantic_context.rs` |
| `method_returns` | Return equations, completed solves, proof outcomes, and telemetry | `inference/method_return.rs` |
| `expressions` | Call outcomes, deferred calls, local-read evidence, and Unknown reasons | `inference/expressions.rs` |
| `constants` | Constant equations and callable bodies | `inference/constants.rs` |

These owners are private to the collector module. They organize temporary
per-file state, not independent semantic databases. The engine handle and query
cache retain their original sharing and lifetime. Mutable flow identities end
with this pass.

Internal cross-family access uses `pub(in crate::indexer::fact_collector)` to
retain that same boundary at every folder depth. Moving a helper into a deeper
folder does not make it visible to the whole crate or to external consumers.

Declaration recording lives in `collection/declarations.rs`; higher-order call
and proc inference lives in `inference/callables.rs`; byte-range and source-comment
helpers live in `context/source.rs`. Literal analysis needs no collector field.

Each extension call frame owns its handled and tracked decisions together.
An untracked nested call leaves its tracked parent in the enclosing-call list;
exiting that nested call must not remove the parent or change its handled state.

## Collection and publication

Create a collector with the document, extension host, and owning engine. Apply
source-specific collection options and namespace inputs, then call
`collector.visit(&parse.node())` through Prism's `Visit` trait. Finally consume
it with `collector.finish()`.

`CollectedFile` contains the updated document, direct facts, collected type
facts, reference and diagnostic candidates, diagnostics, extension patches,
execution contexts, inference evidence, and compact local-read types. It exposes
owned domain values; traversal stacks and mutable stores cannot escape.

Finishing packages existing evidence. It does not traverse again, rerun the
solver, or publish to the engine. The file processor merges the declaration
seed and extension/runtime facts, applies source-kind policy, and uses ordinary
per-file replacement. The simulation engine uses this same completion API.
The reference-query scope rebuild uses `into_document()` because it needs only
the updated scopes, without creating unused proof snapshots.

## Extending collection

Use a node module for syntax-specific behavior. Put reusable declaration,
flow, expression, or callable logic beside that responsibility. Add new state
to its owner only when an existing fact or query cannot represent it.

Extension/runtime hosts read `document()`, `scope_tracker()`,
`extension_project_context()`, `enclosing_extension_calls()`, and fact views.
They contribute through explicit operations such as `add_symbol_fact`,
`add_reference_candidate`, `add_type_fact`, and `record_extension_patch`.
Direct-declaration helpers retain their existing provenance and lookup policies.
Do not expose a mutable owner or storage handle to make a caller compile.

Use the existing feature and simulation tests for observable behavior. Focused
collector regressions in `tests.rs` cover traversal and evidence boundaries,
including nested extension calls, same-pass namespace visibility, shape
invalidation, and execution-context ownership.
