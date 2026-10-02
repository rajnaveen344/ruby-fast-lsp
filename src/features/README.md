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
| `editing/`     | Completion (constant, method, variable, snippet candidates), signature help, rename, code actions, formatting |
| `diagnostics/`  | Engine diagnostics for a document (`engine_diagnostics` over one `&View`); `run_linter`, the lifecycle entry point that lints and retains output for the current source; the external linter and formatter runner |
| `debug.rs`      | FQN lookup, graph export, and extension status requests |
| `presentation/` | Hover, inlay hints, code lenses, document symbols, folding ranges, selection ranges, semantic tokens |

## Contract

- A feature exposes `handle(server, params)`. The `lsp` layer reaches a
  feature only through its `handle` functions, the request types they take
  and return, static capability descriptors, and the one lifecycle entry
  point `diagnostics::run_linter`.
- A static capability descriptor stays beside the encoder it describes; the
  semantic-token legend in `presentation/semantic_tokens.rs` is the
  encoder's own token table, so `lsp/lifecycle` reads it from there rather
  than holding a copy.
- Published diagnostics are composed once by the server
  (`compose_diagnostics`: syntax, engine, then retained linter output). The
  composition stays server-owned because the server's own load and require
  refresh publish too, and `server` never names `features`. `diagnostics/`
  owns the engine projection's feature name, the linter run that feeds the
  retained output, and the linter runner itself.
- A feature may read `server` state and `loader` products. It never names
  `lsp`; an item a feature needs from `lsp` moves to its proper owner.
- Query adapters convert cursor positions to analysis offsets and domain
  ranges to protocol ranges (`cursor::analysis_location`, which shares
  `utils::lsp::lsp_file_range` with diagnostic publication). They do not
  duplicate MRO, identity, ranking, or missing-method policy.
- Split a feature file that grows past 1,000 lines by responsibility inside
  the feature's folder.
