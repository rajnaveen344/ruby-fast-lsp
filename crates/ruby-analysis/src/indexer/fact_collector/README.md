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
  tests/                 Traversal and evidence regressions by responsibility
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
| `semantics` | Read-only project `Semantics` walk handle (it carries the per-pass lookup memo) and same-pass lookup inputs | `context/semantic_context.rs` |
| `method_returns` | Return equations, completed solves, proof outcomes, and telemetry | `inference/method_return.rs` |
| `expressions` | Call outcomes, deferred calls, local-read evidence, and Unknown reasons | `inference/expressions.rs` |
| `constants` | Constant equations and callable bodies | `inference/constants.rs` |

These owners are private to the collector module. They organize temporary
per-file state, not independent semantic databases. The walk's lookup memo
lives behind its `Semantics` handle and is shared by the collector and every
tracker it builds for one pass. Mutable flow identities end with this pass.

The collector and every `TypeTracker` it builds read other files only through
the read-only `inference::semantics::Semantics` trait, never through the engine
lock. The collector reads through `Semantics::for_walk`, which the shared engine
implements with one short read guard per call and one lookup memo per walk, so
no guard spans the walk. The walk never writes the engine. Its reads
either decide which facts get emitted or feed local flow; any new mid-walk read
must become an equation or be added to `Semantics` with a reason. Extension
hosts read through `FactCollector::extension_call_callees`,
`project_namespace_exists`, and `project_type_facts_for`.

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

Create a collector with the document, extension host, and the owning engine
handle, which it reads only as `Semantics`. Apply
source-specific collection options and namespace inputs, then call
`collector.visit(&parse.node())` through Prism's `Visit` trait. Finally consume
it with `collector.finish()`.

The collector keeps every file-owned fact in one `FileAnalysis` while it
traverses. `FactCollectorOutput` holds that `analysis` (declarations, reference
and diagnostic candidates, diagnostics, execution contexts, inference evidence,
and compact local-read types) plus three values that stay outside it: the flow
type facts, the extension patches, and the updated document. It exposes owned
domain values; traversal stacks and mutable stores cannot escape.

Finishing packages existing evidence. It does not traverse again, rerun the
solver, or publish to the engine. The file processor's
`compose_file_analysis` is the one consumer for interactive and batch
indexing. When it has a declaration seed, `FileAnalysis::replace_declarations`
swaps it in and returns the collector's declarations for the
execution-context and runtime merges. It then adds extension facts, merges
flow types by slot, and applies source-kind policy before ordinary per-file
replacement.
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

Use the existing feature and lifecycle tests for observable behavior. Focused
collector regressions in `tests/` cover traversal and evidence boundaries,
including nested extension calls, same-pass namespace visibility, shape
invalidation, and execution-context ownership.
