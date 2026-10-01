# Analysis engine

`mod.rs` preserves the public API: register source and replace a file's
`core::FileAnalysis` through `AnalysisEngine`, then read domain results through
`AnalysisQuery`. The engine has no per-file type of its own; `replace_facts`
takes the core value.
Implementation folders are private to the engine.

| Area | Responsibility |
| --- | --- |
| `state/` | `AnalysisEngine` and its components (see below); `lifecycle` orders file replacement and resolve passes over them, and `mod` owns the engine identity, `semantic_revision`, `query_cache_identity`, statistics, memory stats, and compaction |
| `persist/fingerprint/` | Semantic export and result fingerprints that classify file replacements and key persistent caches |
| `persist/external_facts_template/` | Project-neutral dependency fact templates and their snapshot codecs |
| `state/tests/` | Engine state tests grouped by lifecycle, fingerprints, inference outcomes, navigation, graph, caches, and constants |
| `resolution/` | Ruby lookup chains and MRO (`lookup_chain`), chain method facts and visibility (`chain_methods`), callees, signatures, method references, reference ranges, definitions, and rename policy |
| `queries/` | Common reads and the `AnalysisQuery` entry point |
| `queries/cache/` | Per-source and thread-local method lookup memos (`memo`, `thread_memo`), and expression, binding, namespace/constant, and method-return type queries |
| `queries/definitions/` | Definition source selection and partial ordering from participating Ruby lookup chains |
| `queries/lookup/` | Constant/method matching and hover lookup results |
| `queries/hierarchy/` | Call/type hierarchy queries and result types |
| `queries/namespace_tree/` | Namespace-tree projections and their result types |
| `queries/workspace_symbols/` | Workspace symbol matching and its result type |
| `diagnostics/` | The `Diagnostics` store of diagnostic candidates and resolved diagnostics with its require swap and resolve rebuild (`store`), reference-candidate resolution passes (`workspace_pass`, `file_pass`), call outcomes, grouped (union-receiver) dispatch, unresolved-method absence proof, resolved-method availability and signature checks, indexer diagnostic candidates, and diagnostic helpers |
| `debug/` | Introspection and storage-size projections |

## State components

`AnalysisEngine` holds one private-field component per kind of semantic state.
Each component owns its fields and exposes `pub(in crate::engine)` operations;
components that need names take `&Names` or `&mut Names` as a parameter. The
engine keeps only its identity, `semantic_revision`, `query_cache_identity`,
and last resolve statistics.

| Component | Module | Owns |
| --- | --- | --- |
| `Names` | `state/names.rs` | The FQN interner and constant lookups |
| `Files` | `state/files.rs` | Sources, file ids, line indexes, revision snapshots, and export fingerprints |
| `DeclIndex` | `state/decls.rs` | Symbols, methods, visibility overrides, execution contexts, and effective-method reads |
| `Hierarchy` | `state/hierarchy.rs` | Graph nodes and ancestry edges, constant path resolution, unresolved-edge retry, and method lookup chain caches |
| `UseIndex` | `state/uses.rs` | Reference candidates and resolved references |
| `Diagnostics` | `diagnostics/store.rs` | Diagnostic candidates and resolved diagnostics |
| `TypeTable` | `state/types.rs` | Type facts, call-expression outcomes, proven local-read types, the resolved-outcome merge, and the writers equation solving applies |
| `Solver` | `state/solver.rs` | Per-file inference evidence and equation dirty flags; constant-type and method-return solves run as a read-only plan followed by an `apply` step that writes the `TypeTable` |

`state/lifecycle.rs` is orchestration only: it advances the semantic revision,
then writes each component in a fixed order and runs the resolve passes.

Query result types live with their query family. Shared helpers retain the
engine-wide privacy boundary even when a folder adds another module level.
Submodules of `state/` and `resolution/` extend `AnalysisEngine` and
`AnalysisQuery` with inherent impl blocks; their helpers stay private to the
owning folder unless a sibling engine area needs them through a narrow re-export.

Every directory stays within the ten-entry ceiling. The split does not merge
stores, change locks, or alter the file-owned lifecycle.
