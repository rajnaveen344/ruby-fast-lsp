# Analysis engine

`mod.rs` preserves the public API: register source and replace `FileFacts` through
`AnalysisEngine`, then read domain results through `AnalysisQuery` or `TypeQuery`.
Implementation folders are private to the engine.

| Area | Responsibility |
| --- | --- |
| `state/` | Source and name registries, fact ownership, file replacement, immutable templates, and lifecycle tests |
| `resolution.rs` | Ruby lookup chains, MRO, visibility, dependency resolution, and rename policy |
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
`state/tests.rs` uses ordinary Rust module discovery beside `state/mod.rs`.

Every directory is below the ten-entry ceiling. The large existing state and
resolution implementations remain separate responsibilities for future internal
cleanup; this layout change does not merge stores, change locks, or alter the
file-owned lifecycle.
