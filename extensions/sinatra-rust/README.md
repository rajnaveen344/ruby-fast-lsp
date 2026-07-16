# Sinatra Rust extension

This bundled Rust-authored extension models Sinatra's application and request
execution scopes through Ruby Fast LSP's public extension ABI. It supports
classic `Sinatra::Application` and modular `Sinatra::Base` applications,
request routes and filters, block-defined helpers, and helper modules.

The adapter changes implicit receiver and method-definition ownership without
changing lexical constant or closure/local scope. It depends only on the public
extension API and typed Rust guest SDK and is compiled to bounded Wasm.

Build the package and run its native and black-box LSP gates:

```bash
extensions/sinatra-rust/build-and-test.sh
```

The manifest is checksum-bound. Update its checksum only after an intentional
guest or SDK change.
