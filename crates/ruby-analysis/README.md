# Ruby analysis library

Reusable Ruby semantics for the language server and standalone tools. The crate
root exposes four modules; import a concept from the module that owns it.

| Module | Responsibility | Main entry points |
| --- | --- | --- |
| `core` | Names, source identities, byte ranges, types, and semantic facts | `RubyType`, `TextRange`, `SourceFileId`, `MethodFact` |
| `indexer` | Parse source and collect file-owned facts | `AnalysisIndexer`, `fact_collector::FactCollector`, `index_rbs` |
| `inference` | Derive types from expressions, flow, calls, and signatures | `type_tracker::TypeTracker`, `method`, `rbs` |
| `engine` | Own project state, resolve facts, and answer semantic queries | `AnalysisEngine`, `FileFacts`, `AnalysisQuery`, `TypeQuery` |

`RubyType` belongs to `core`, including when inference produces it. Engine
stores, interned IDs, and stored representations are internal. Public consumers
submit domain facts and read query results; they cannot obtain storage handles.

## Reading the code

Follow a file through `indexer` (collection), `inference` (type derivation), and
`engine` (replacement, resolution, queries). The engine coordinates cross-file
constant and method-return equation solvers in `inference`; those solvers may
consult engine queries. These are cooperating modules in one crate. The engine
owns the solved state and Ruby lookup policy; inference owns the type rules.

Use the [core guide](src/core/README.md) for contracts and compact stores,
the [indexer guide](src/indexer/README.md) for documents and parser queries,
and the [engine guide](src/engine/README.md) for lifecycle, resolution, and query
families. Every folder in this library meets the ten-entry rule; the automated
source-layout check rejects legacy allowances inside this crate.

For local inference, start with `TypeTracker::new()` and follow its existing
Prism tree through traversal, expressions, flow, and method return solving. The
[tracker guide](src/inference/type_tracker/README.md) maps its private state
owners and explains how observations return to the collector.

The server selects projects, schedules work, supplies extension/runtime facts,
and converts domain byte ranges to editor positions outside this library.

## Register, collect, replace, query

This declaration-only example uses `AnalysisIndexer`. Full semantic collection
also uses `FactCollector` for body inference, references, diagnostics, and
extension hooks. After traversal, `FactCollector::finish()` returns an owned
`CollectedFile`; the server composes it into the same `FileFacts`. See the
[collector guide](src/indexer/fact_collector/README.md) for its private state
owners and traversal flow.

```rust
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::{AnalysisEngine, FileFacts, ResolveMode, SourceFileInput};
use ruby_analysis::indexer::AnalysisIndexer;

let mut engine = AnalysisEngine::new();
let source = "class Lantern; end";
let file_id = engine.register_file(SourceFileInput {
    path: "lantern.rb".into(),
    content: source.into(),
    kind: SourceKind::Project,
});
let collected = AnalysisIndexer::new(file_id).index_source(source);
engine.replace_facts(file_id, FileFacts {
    symbols: collected.symbols,
    methods: collected.methods,
    method_visibility_overrides: collected.method_visibility_overrides,
    graph_nodes: collected.graph_nodes,
    graph_edges: collected.graph_edges,
    unresolved_graph_edges: collected.unresolved_graph_edges,
    types: collected.types,
    ..FileFacts::default()
}, ResolveMode::Immediate);

assert_eq!(engine.query().symbol_facts_in_file(file_id).len(), 1);

// Replacing a file also removes its old semantic facts.
let edited_id = engine.register_file(SourceFileInput {
    path: "lantern.rb".into(),
    content: String::new(),
    kind: SourceKind::Project,
});
assert_eq!(edited_id, file_id);
engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
assert!(engine.query().symbol_facts_in_file(file_id).is_empty());
```

Use deferred replacement followed by one `engine.resolve()` for a batch. Delayed
background producers must retain `SourceFileSnapshot` and use
`replace_facts_if_source_snapshot`; an obsolete producer cannot replace newer
source facts.

`AnalysisQuery` reads an engine snapshot. `TypeQuery::new(&engine, file_id)` is a
file-scoped view of existing type facts; it does not trigger inference or copy
stores. During collection, extensions use `FactCollector::add_type_fact` and
the collector's fact views. Final publication still goes through replacement.

## Storage stays internal

These examples intentionally fail to compile: library consumers cannot import
stores or obtain them through the engine or collector.

```compile_fail
use ruby_analysis::core::TypeStore;
```

```compile_fail
use ruby_analysis::engine::AnalysisEngine;
let engine = AnalysisEngine::new();
let storage = engine.type_store();
```

```compile_fail
use ruby_analysis::indexer::fact_collector::FactCollector;
fn inspect(collector: &FactCollector) {
    let state = &collector.facts;
}
```

Reference results expose source ranges and access information. Caller identity
is resolved inside engine queries, including call hierarchy, rather than exposing
an engine-local interned ID.

```compile_fail
use ruby_analysis::core::ReferenceFact;
fn inspect(reference: &ReferenceFact) {
    let internal_caller = reference.caller;
}
```

## Extending the library

Add shared fact contracts in `core`, collect them in `indexer`, put type rules
in `inference`, and expose persistent results through `engine`. Export only
the operations and records callers need. Start with a focused semantic test,
then use the server's lifecycle and simulation tests when file replacement,
project ownership, or asynchronous publication is involved.
