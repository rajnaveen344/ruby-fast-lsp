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
