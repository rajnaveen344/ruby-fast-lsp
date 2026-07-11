# Ruby Fast LSP mruby SDK

Tiny Ruby DSL and reusable build toolchain for packaging Ruby Fast LSP
extensions as sandboxed mruby Wasm guests.

The SDK intentionally exposes only pure transform helpers:

```text
CallContext -> IndexPatch[]
```

No extension gets direct access to the analysis engine.

## Start from the example

Copy `extensions/example-dsl`, change its manifest ID, and implement handlers
in `extension.rb`. The example demonstrates generated namespace, typed
constant, and method semantic patches, document symbols, code lenses,
source-level tests, and black-box LSP acceptance. The corresponding SDK helpers
are `define_namespace`, `define_constant`, `define_method`, `set_superclass`, and
`apply_mixin`. `set_superclass` is accepted only alongside the matching
generated class declaration from the same callback, so an extension cannot
override parser-owned inheritance.
Generated method returns and constants can use `named_type`, `array_type`,
`hash_type`, `union_type`, and `nilable_type`; these produce structured ABI
types rather than strings that the server would need to parse.
Use `unknown_type` when a framework declaration intentionally cannot identify
a concrete Ruby type.
DSL tokens can become semantic references through `add_reference` with
`namespace_reference_target`, `constant_reference_target`, or
`method_reference_target`. Exact method targets still use the engine's normal
MRO, ambiguity, visibility, definition, and find-references policy; guests do
not resolve method facts themselves. The server owns the resulting reference
index and query behavior.

Call arguments preserve literal values and source ranges. Ruby keyword pairs
are flattened into ordinary `Argument` objects with `keyword_name`,
`keyword_range`, and the value `range`; use `ctx.keyword_argument("name")` for
exact option lookup. This is additive to the ABI: positional arguments and
older guest payloads continue to omit keyword metadata.

Lexical DSLs may declare `[indexing].frame_call_names` independently from guest
handler `call_names`. Matching calls are retained in `ctx.enclosing_calls`, and
each frame includes the same literal/keyword `arguments` contract. This lets a
guest interpret nested DSL scope without adding fake semantic methods or
hard-coding framework frames in the core. Frame names must be valid Ruby method
names; guests must still verify that a child call belongs to their intended
root frame before emitting patches.

Runtime-backed extensions may return `reindex_files` from
`on_process_completed`. Each entry names an event-related `workspace_root` and
a workspace-relative `path`. The host validates and bounds these requests, then
routes them through normal file reindexing; cached runtime knowledge can affect
semantic patches only when ordinary call hooks run again.

Every package contains `extension.toml`, `extension.rb`, `runtime.rb`, and the
built Wasm path declared by its manifest.

## Build

With local mruby and WASI SDK installations:

```bash
MRUBY_ROOT=/path/to/mruby \
WASI_SDK_PATH=/path/to/wasi-sdk \
extensions/mruby-sdk/scripts/build-wasm.sh extensions/example-dsl
```

Or use the reproducible Docker builder:

```bash
extensions/mruby-sdk/scripts/build-wasm-docker.sh extensions/example-dsl
```

The builder reads the package ID and output path from `extension.toml`, bundles
the SDK/source/runtime, generates a package-specific mruby bytecode symbol, and
links the common JSON ABI shim. It contains no RSpec- or server-specific code.

Validate the result with:

```bash
cargo run --bin extension -- validate extensions/example-dsl
```
