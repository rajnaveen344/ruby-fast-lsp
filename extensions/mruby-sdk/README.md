# Ruby Fast LSP mruby SDK

Tiny Ruby DSL for writing extension logic that can later be packaged into an
mruby Wasm component.

The SDK intentionally exposes only pure transform helpers:

```text
CallContext -> IndexPatch[]
```

No extension gets direct access to the analysis engine.
