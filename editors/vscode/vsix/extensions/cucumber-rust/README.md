# Cucumber Rust extension

This bundled Rust-authored extension models Cucumber-Ruby's per-scenario World
through Ruby Fast LSP's public execution-context ABI. Step definitions and
scenario hooks use a project-scoped hidden World receiver while retaining their
source lexical constants, local closures, and Ruby method-definition owner.
`World(SomeModule)` contributes ordinary mixins to that receiver. A `World`
factory block remains in its ordinary lexical context.

Build the checksum-bound Wasm and run native plus black-box LSP gates:

```bash
extensions/cucumber-rust/build-and-test.sh
```
