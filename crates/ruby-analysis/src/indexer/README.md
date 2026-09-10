# Parser and fact collection

The public API is exported from `mod.rs`. Follow a source document through
declaration collection, the stateful `FactCollector` pass, and the caller's
composition into engine facts. Scheduling and publication stay outside indexer.

| Area | Responsibility |
| --- | --- |
| `documents/` | Source text/offsets, ERB mapping, Ruby documents, scopes, and local variables |
| `lowering/` | Declaration and RBS indexing plus compact callable/block fact construction |
| `fact_collector/` | Stateful Prism traversal and body/reference/diagnostic evidence |
| `identifiers/` | Position-based identifier discovery and identifier/receiver types |
| `queries/` | Syntax-backed hover, receiver lookup, rename targets, symbols, lenses, selections, and token queries |
| `inlay_hints.rs` | Reusable inlay-hint evidence collection |
| `yard/` | YARD parsing and its type contracts |

`identifiers/mod.rs` owns the visitor and shared context. Its node callbacks are
grouped into `calls`, `constants`, `declarations`, `variables`, and `scope`.
These groups preserve traversal and private visitor state; they do not define
another lookup policy. Identifier-related domain types live in `identifiers/types.rs`.

`queries/mod.rs` owns `RubyPrismAnalyzer`. Its `syntax` helpers handle Prism
names and locations; other query modules emit reusable domain results. They do
not construct LSP responses. Keep engine-owned resolution in the engine.

All directories meet the ten-entry limit. The query folder is at the limit;
another query file needs a meaningful subdivision. For collector state and
traversal details, continue with `fact_collector/README.md`.
