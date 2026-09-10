# Core contracts

Start at `mod.rs` for the public domain exports. Consumers still import types
such as `RubyType`, `TextRange`, and `MethodFact` directly from `core`.
Implementation modules and stored representations remain crate-private.

| Area | Responsibility |
| --- | --- |
| `names/` | Ruby names, namespaces, fully qualified names, and interned identities |
| `source/` | Source ownership, coordinates, and lexical execution context |
| `types/` | Canonical Ruby values, structural shapes, and proof outcomes |
| `callables/` | AST-free callable bodies and parameter/signature contracts |
| `equations/` | Constant and method-return dependency equations |
| `storage/` | Compact file-owned stores, insertion helpers, and memory accounting |
| `method_resolution.rs` | Domain result of resolving a callable method |

Core does not traverse Prism trees, schedule indexing, or decide Ruby lookup
policy. The engine owns persistent state and replacement. Keep source identities,
type contracts, and compact storage separate when adding a new concept.

All directories meet the ten-entry limit. Storage is at the limit; a new storage
module needs a meaningful subdivision rather than another flat entry.
