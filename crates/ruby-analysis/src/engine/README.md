# Analysis engine

`mod.rs` preserves the public API: register source and replace a file's
`core::FileAnalysis` through `Project`, then read domain results through
`View`. `AnalysisEngine` and `AnalysisQuery` remain as type aliases of
`Project` and `View` until the remaining callers migrate. The engine has no
per-file type of its own; `update` takes the core value. `remove` (and
`remove_if_snapshot`) drops a file from every component and from `Files`;
file ids are never reissued, so snapshots of a removed file stay stale.
Implementation folders are private to the engine.

| Area | Responsibility |
| --- | --- |
| `state/` | `Project` and its components (see below); `lifecycle` orders file replacement, file removal, and resolve passes over them, and `mod` owns the engine identity, `semantic_revision`, `query_cache_identity`, statistics, memory stats, and compaction |
| `persist/fingerprint/` | Semantic export and result fingerprints that classify file replacements and key persistent caches |
| `persist/external_facts_template/` | Project-neutral dependency fact templates and their snapshot codecs |
| `state/tests/` | Engine state tests grouped by lifecycle, removal, fingerprints, inference outcomes, navigation, graph, caches, and constants |
| `lookup/` | `lookup::method(view, MethodRequest)` and `method_cached`: one request (receiver, method, `ReceiverAccess`, wanted product) answered as `MethodAnswer::{Found, Ambiguous, Missing, Unknown}`. `Unknown` means an unknown lookup edge (unindexed receiver, unresolved ancestor, no proven product, or an unsupported want) and suppresses missing-method claims; only `Missing` proves absence. `MethodLookupResult` (a method reference) is `MethodAnswer<Arc<MethodFact>>`, and the per-owner `EffectiveMethodFactMatch` is `MethodAnswer<MethodFact, Infallible>`, since one owner has no unknown edge. The access-flavoured callee, return-type, and signature wrappers on `View` are one-line views of `method`/`method_cached`; cached wrappers keep the existing memo maps. The receiver return-type walk reads each own-chain winner through a `Reflection` `Facts` lookup after dispatching to module includers itself. Answers are derived data and never enter fingerprints |
| `resolution/` | Ruby lookup chains and MRO (`lookup_chain`), chain method facts and visibility (`chain_methods`), callees, signatures, method references, reference ranges, definitions, and rename policy |
| `queries/` | Common reads and the `View` entry point |
| `semantics.rs` | The engine's implementations of `inference::semantics::Semantics`, the read-only trait the fact collector and `TypeTracker` use for mid-walk reads: one for `View` (which uses its walk memo when it has one) and one for the shared engine lock and its per-walk handle (`ProjectWalk`, which owns the walk's `AnalysisQueryCache`), which takes one short read guard per call and delegates to a `View` |
| `queries/cache/` | Per-source and thread-local method lookup memos (`memo`, `thread_memo`), and expression, binding, namespace/constant, and method-return type queries |
| `queries/definitions/` | Definition source selection and partial ordering from participating Ruby lookup chains |
| `queries/completion/` | Completion receiver/type probing over documents, exported as `engine::completion`; editor trigger routing and snippets stay in the server |
| `queries/lookup/` | Constant/method matching and hover lookup results |
| `queries/hierarchy/` | Call/type hierarchy queries and result types |
| `queries/namespace_tree/` | Namespace-tree projections and their result types |
| `queries/workspace_symbols/` | Workspace symbol matching and its result type |
| `diagnostics/` | The `Diagnostics` store of diagnostic candidates and resolved diagnostics with its require swap and resolve rebuild (`store`), reference-candidate resolution passes (`workspace_pass`, `file_pass`), call outcomes, grouped (union-receiver) dispatch, unresolved-method absence proof, resolved-method availability and signature checks, indexer diagnostic candidates, and diagnostic helpers |
| `debug/` | Introspection and storage-size projections |

## State components

`Project` holds one private-field component per kind of semantic state.
Each component owns its fields and exposes `pub(in crate::engine)` operations;
components that need names take `&Names` or `&mut Names` as a parameter. The
engine keeps only its identity, `semantic_revision`, `query_cache_identity`,
and last resolve statistics.

| Component | Module | Owns |
| --- | --- | --- |
| `Names` | `state/names.rs` | The FQN interner and constant lookups |
| `Files` | `state/files.rs` | Sources, file ids, line indexes, revision snapshots, and export fingerprints |
| `DeclIndex` | `state/decls.rs` | Symbols, methods, visibility overrides, execution contexts, and effective-method reads |
| `Hierarchy` | `state/hierarchy.rs` | Graph nodes and ancestry edges, constant path resolution, unresolved-edge retry (with records that return a retried edge to unresolved when its target loses its last definition), and method lookup chain caches |
| `UseIndex` | `state/uses.rs` | Reference candidates and resolved references |
| `Diagnostics` | `diagnostics/store.rs` | Diagnostic candidates and resolved diagnostics |
| `TypeTable` | `state/types.rs` | Type facts, call-expression outcomes, proven local-read types, the resolved-outcome merge, and the writers equation solving applies |
| `Solver` | `state/solver.rs` | Per-file inference evidence and equation dirty flags; constant-type and method-return solves run as a read-only plan followed by an `apply` step that writes the `TypeTable` |

`state/lifecycle.rs` is orchestration only: it advances the semantic revision,
then writes (or, for `remove`, empties) each component in a fixed order and
runs the resolve passes. Removing an unknown file id changes nothing.

Query result types live with their query family. Shared helpers retain the
engine-wide privacy boundary even when a folder adds another module level.
Submodules of `state/` and `resolution/` extend `Project` and
`View` with inherent impl blocks; their helpers stay private to the
owning folder unless a sibling engine area needs them through a narrow re-export.

## Mid-walk reads

A file walk (the fact collector and `TypeTracker`) never holds the engine lock.
It reads other files through `inference::semantics::Semantics`, whose methods each answer one
question and return plain domain values. They come in two kinds: reads that
decide which facts get emitted (namespace or singleton receivers, `initialize`
inside a class, extension call targets) and reads that feed local flow (RBS
contracts, higher-order block parameters, callable constant bodies, dispatched
method returns, `super`, and constructors). Everything else crosses files as an
equation solved after replacement. Any new mid-walk read must become an
equation or be added to `Semantics` with a reason. The walk never writes the
engine. A third, small group serves editor receiver resolution
(`indexer::resolve_receiver_type`), which a server query runs over a `View`.

Every directory stays within the ten-entry ceiling. The split does not merge
stores, change locks, or alter the file-owned lifecycle.
