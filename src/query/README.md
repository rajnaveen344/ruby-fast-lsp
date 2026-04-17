# Query Engine

The `query` module provides a unified interface for querying the `RubyIndex`. It consolidates complex business logic, abstracting away the low-level index details.

```text
server.rs (API) → query/ (Service) → indexer/ (Data)
```

## Public API

The `IndexQuery` struct provides **4 unified position-based APIs**:

| Feature         | Method                                                       | Returns         |
| :-------------- | :----------------------------------------------------------- | :-------------- |
| **Definitions** | `find_definitions_at_position(uri, pos, content, narrowing)` | `Vec<Location>` |
| **References**  | `find_references_at_position(uri, pos, content)`             | `Vec<Location>` |
| **Hover**       | `get_hover_at_position(uri, pos, content, narrowing)`        | `HoverInfo`     |
| **Types**       | `resolve_type_at_position(uri, pos, content, narrowing)`     | `RubyType`      |

Each method handles all identifier types internally (constants, methods, local/instance/class/global variables, YARD types).

## Usage

```rust
use crate::query::IndexQuery;

// Create query with document context
let query = IndexQuery::with_doc(&index, &document);

// Use the unified APIs
let defs = query.find_definitions_at_position(&uri, pos, &content, Some(&narrowing));
let refs = query.find_references_at_position(&uri, pos, &content);
let hover = query.get_hover_at_position(&uri, pos, &content, Some(&narrowing));
let ty = query.resolve_type_at_position(&uri, pos, &content, Some(&narrowing));
```

## Shared Types

- **`HoverInfo`**: Content, range, and inferred type for hover requests.
- **`MethodInfo`**: FQN, visibility, return type, and documentation for a method.

## Contract with `capabilities/`

The split between `src/capabilities/` and `src/query/` is intentional. Keep it this way.

| Layer | Owns | Imports `tower_lsp::lsp_types`? |
| :--- | :--- | :--- |
| `capabilities/*.rs` | LSP handler: URI → doc lookup → build `IndexQuery` → format result as LSP types | **Yes** |
| `query/*.rs` | Index-backed read logic returning domain types (`FullyQualifiedName`, `RubyType`) | **No** (except `Location`/`Position` re-used directly) |

**Rules:**

1. `query/` must not depend on `RubyLanguageServer`, the `docs` map, or handler-specific plumbing. It takes an index + (optionally) a document Arc.
2. `capabilities/` adapter files stay thin (~20–120 lines). If a capability file grows past ~150 lines while backed by the index, the extra logic probably belongs in `query/`.
3. **Exception:** features that don't need the index live only in `capabilities/`. Current examples:
   - `capabilities/formatting.rs` — runs an external formatter
   - `capabilities/folding_range.rs` — pure AST visitor
   - `capabilities/semantic_tokens.rs` — pure AST visitor
   - `capabilities/document_symbols.rs` — pure AST visitor
   No query counterpart is needed for these.
4. New LSP feature that needs the index → create both `capabilities/foo.rs` (adapter) and `query/foo.rs` (logic). Do not put index-backed logic in `capabilities/`.

**Why the split matters:** `query/` is reusable from tests, CLI binaries (`src/bin/profile_*`), and future non-LSP frontends. Leaking `tower_lsp` types into `query/` would couple it to the protocol.
