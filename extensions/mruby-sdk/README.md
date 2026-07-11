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
are `define_namespace`, `define_constant`, `define_method`, and `apply_mixin`.

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
