# Parser and fact collection

The public API is exported from `mod.rs`. Follow a source document through
declaration collection, the stateful `FactCollector` pass, and the caller's
composition into engine facts. Scheduling and publication stay outside indexer.

| Area | Responsibility |
| --- | --- |
| `documents/` | Source text/offsets, ERB mapping, Ruby documents, scopes, and local variables |
| `lowering/` | Declaration and RBS indexing plus forwarded-block fact construction; callable-literal lowering lives in `inference/callable_body/` |
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
Receiver resolution (`queries/receivers.rs`) and the fact collector read project
state only through `inference::semantics::Semantics`; outside tests the indexer
names no engine type, and callers pass a `View` or the shared engine.

Scope rules shared by every walk (lexical constant lookup, declaration reopen
candidates, the class or module a constant alias reopens, the namespace a static receiver names, and the execution context
an eval, `define_method`, or Concern `class_methods` block opens) live in
`documents/scope_rules.rs`, beside `ScopeTracker`. A walk passes in only which
namespaces it knows or how it resolves a receiver; do not copy a rule into a
walker. Inside an eval block, definitions follow the receiver while constant
writes and nested `class`/`module` bodies stay in the lexical scope.

All directories meet the ten-entry limit. The query folder is at the limit;
another query file needs a meaningful subdivision. For collector state and
traversal details, continue with `fact_collector/README.md`.
