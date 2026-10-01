# Analysis engine

`mod.rs` preserves the public API: register source and replace `FileFacts` through
`AnalysisEngine`, then read domain results through `AnalysisQuery` or `TypeQuery`.
Implementation folders are private to the engine.

| Area | Responsibility |
| --- | --- |
| `state/` | `AnalysisEngine` ownership, source and name registries (`storage`), file replacement and resolve passes (`lifecycle`), fact reads (`facts`), graph reads and lookup caches (`graph`), stored inference outcomes (`inference`), and semantic fingerprints (`fingerprint/`) |
| `state/external_facts_template/` | Project-neutral dependency fact templates and their snapshot codecs |
| `state/tests/` | Engine state tests grouped by lifecycle, fingerprints, inference outcomes, navigation, graph, caches, and constants |
| `resolution/` | Ruby lookup chains and MRO (`lookup_chain`), chain method facts and visibility (`chain_methods`), callees, signatures, method references, reference ranges, definitions, and rename policy |
| `queries/` | Common reads, query caching, and file-scoped type queries |
| `queries/definitions/` | Definition source selection and partial ordering from participating Ruby lookup chains |
| `queries/lookup/` | Constant/method matching and hover lookup results |
| `queries/hierarchy/` | Call/type hierarchy queries and result types |
| `queries/namespace_tree/` | Namespace-tree projections and their result types |
| `queries/workspace_symbols/` | Workspace symbol matching and its result type |
| `diagnostics/` | Reference-candidate resolution, diagnostic derivation, and diagnostic helpers |
| `debug/` | Introspection and storage-size projections |

Query result types live with their query family. Shared helpers retain the
engine-wide privacy boundary even when a folder adds another module level.
Submodules of `state/` and `resolution/` extend `AnalysisEngine` and
`AnalysisQuery` with inherent impl blocks; their helpers stay private to the
owning folder unless a sibling engine area needs them through a narrow re-export.

Every directory stays within the ten-entry ceiling. The split does not merge
stores, change locks, or alter the file-owned lifecycle.
