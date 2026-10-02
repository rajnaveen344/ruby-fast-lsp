# Editor features

Each feature owns one editor capability end to end: the request body, cursor
and document context, the query adapter over `ruby-analysis`, and conversion
to protocol values. Reusable Ruby semantics stay in `ruby-analysis::engine`;
a feature only adapts them.

```text
lsp/service -> features::<family>::<feature>::handle -> ruby-analysis engine
```

## Layout

| Module        | Owns                                                                                                                |
| :------------ | :------------------------------------------------------------------------------------------------------------------ |
| `cursor/`     | `EngineQuery` (document plus owning engine), method lookup, range conversion                                        |
| `navigation/` | Definition (with indexing demand waits), implementation, references, document highlights, type and call hierarchies, workspace symbols, namespace tree |

## Contract

- A feature exposes `handle(server, params)`. The `lsp` layer reaches a
  feature only through its `handle` functions, the request types they take
  and return, and static capability descriptors.
- A feature may read `server` state and `loader` products. It never names
  `lsp`; an item a feature needs from `lsp` moves to its proper owner.
- Query adapters convert cursor positions to analysis offsets and domain
  ranges to protocol ranges (`cursor::analysis_location`). They do not
  duplicate MRO, identity, ranking, or missing-method policy.
- Split a feature file that grows past 1,000 lines by responsibility inside
  the feature's folder.
