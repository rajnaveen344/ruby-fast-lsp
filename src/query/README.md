# LSP Query Adapters

The `query` module adapts editor requests to `ruby-analysis::engine`. It owns cursor
parsing, document context, and protocol conversion. Reusable domain logic belongs
in `crates/ruby-analysis/src/engine`.

```text
server.rs (LSP) -> query/ (adapter) -> ruby-analysis::engine (domain)
```

## Public API

The `EngineQuery` struct provides position-based LSP-facing APIs:

| Feature         | Method                                                       | Returns         |
| :-------------- | :----------------------------------------------------------- | :-------------- |
| **Definitions** | `find_definitions_at_position(uri, pos, content, narrowing)` | `Vec<Location>` |
| **References**  | `find_references_at_position(uri, pos, content)`             | `Vec<Location>` |
| **Hover**       | `get_hover_at_position(uri, pos, content, narrowing)`        | `HoverInfo`     |
| **Types**       | `resolve_type_at_position(uri, pos, content, narrowing)`     | `RubyType`      |

Each method handles protocol context internally, then delegates fact/graph/type
queries to `AnalysisQuery`.

## Usage

```rust
use crate::query::EngineQuery;

// Create query with document context and engine access
let query = EngineQuery::with_doc_and_engine(document, analysis_engine);

// Use the unified APIs
let defs = query.find_definitions_at_position(&uri, pos, &content);
let refs = query.find_references_at_position(&uri, pos, &content);
let hover = query.get_hover_at_position(&uri, pos, &content);
let ty = query.resolve_type_at_position(&uri, pos, &content);
```

## Shared Types

- **`HoverInfo`**: Content, range, and inferred type for hover requests.
- **`MethodInfo`**: FQN, visibility, return type, and documentation for a method.

## Contract with `capabilities/`

The split between `src/capabilities/` and `src/query/` is intentional. Keep it this way.

| Layer | Owns | Imports `tower_lsp::lsp_types`? |
| :--- | :--- | :--- |
| `capabilities/*.rs` | LSP handler: URI -> doc lookup -> build `EngineQuery` -> format result as LSP types | **Yes** |
| `query/*.rs` | LSP adapter over `AnalysisQuery`: cursor parsing, document context, `TextRange -> Location` | **Yes, only at adapter boundary** |
| `crates/ruby-analysis/src/engine` | Reusable domain queries over facts, graph, references, types | **No** |

**Rules:**

1. `query/` must not depend on `RubyLanguageServer`, the `docs` map, or handler-specific plumbing. It takes `AnalysisEngine` + optional document context.
2. `capabilities/` adapter files stay thin (~20–120 lines). If a capability file grows past ~150 lines while backed by analysis facts, the extra logic probably belongs in `query/` or, if editor-agnostic, `ruby-analysis::engine`.
3. **Exception:** features that don't need analysis facts live only in `capabilities/`. Current examples:
   - `capabilities/formatting.rs` — runs an external formatter
   - `capabilities/folding_range.rs` — pure AST visitor
   - `capabilities/semantic_tokens.rs` — pure AST visitor
   - `capabilities/document_symbols.rs` — pure AST visitor
   No query counterpart is needed for these.
4. New LSP feature that needs project facts -> put domain query in `ruby-analysis::engine`, then add `query/foo.rs` for protocol conversion and `capabilities/foo.rs` for handler plumbing.

**Why the split matters:** engine is reusable from tests, CLI binaries, and future
non-LSP frontends. `query/` is allowed to speak LSP because it is the editor
adapter.
