# Analysis engine

`mod.rs` preserves the public API: register source and replace a file's
`core::FileAnalysis` through `AnalysisEngine`, then read domain results through
`AnalysisQuery`. The engine has no per-file type of its own; `replace_facts`
takes the core value.
Implementation folders are private to the engine.

| Area | Responsibility |
| --- | --- |
| `state/` | `AnalysisEngine` ownership, the `Names` interner for FQNs and constant lookups (`names`), the `Files` registry of sources, file ids, line indexes, revision snapshots, and export fingerprints (`files`), the fact arena and fact interning (`storage`), file replacement and resolve passes (`lifecycle`), fact reads (`facts`), graph reads and lookup caches (`graph`), and stored inference outcomes (`inference`) |
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
| `diagnostics/` | Reference-candidate resolution passes (`workspace_pass`, `file_pass`), call outcomes, grouped (union-receiver) dispatch, unresolved-method absence proof, resolved-method availability and signature checks, indexer diagnostic candidates, and diagnostic helpers |
| `debug/` | Introspection and storage-size projections |

Query result types live with their query family. Shared helpers retain the
engine-wide privacy boundary even when a folder adds another module level.
Submodules of `state/` and `resolution/` extend `AnalysisEngine` and
`AnalysisQuery` with inherent impl blocks; their helpers stay private to the
owning folder unless a sibling engine area needs them through a narrow re-export.

Every directory stays within the ten-entry ceiling. The split does not merge
stores, change locks, or alter the file-owned lifecycle.
