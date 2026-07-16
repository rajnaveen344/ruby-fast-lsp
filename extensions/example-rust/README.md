# Example Rust extension

This package is the public-contract acceptance consumer for the typed Rust
guest SDK. It deliberately depends only on `extension-api` and
`extension-guest-sdk`, compiles to `wasm32-wasip1`, and is loaded through the
same manifest, validation, resource limits, lifecycle, and fact-ingestion path
as Ruby/mruby guests.

Build and run both the Wasmtime contract test and black-box LSP test:

```bash
extensions/example-rust/build-and-test.sh
```

The gate fails when rebuilt bytes do not match the checksum-bound manifest.
After an intentional SDK or guest change, update `checksum_sha256` and rerun
the gate.
