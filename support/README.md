# Support assets and validation

These are product inputs, executable checks, and a small set of retained
engineering decisions. Human-facing guides live in [docs/](../docs/README.md).

| Directory | Why it remains |
| --- | --- |
| [jruby/](jruby/) | Runtime stubs and the licensed CFR decompiler used by the server and packaged into npm/VSIX |
| [structure/](structure/README.md) | Active source-folder policy, checker, and checker tests |
| [type_inference/](type_inference/) | Reviewed `scorecard.toml` and `real_project_precision.toml` acceptance contracts loaded by tests |
| [performance/](performance/README.md) | Curated evidence explaining current inference bounds and instrumentation decisions |

Keep files here only when a product path, automated check, or maintained contract
needs them. Ordinary timing logs, copied campaign results, and superseded progress
reports belong in build/release artifacts, not the working tree. Committed older
reports remain in Git history. New local output goes under `target/`.

Do not remove runtime assets or acceptance contracts based on age alone. Use the [release runner](../docs/development/release.md)
for broad validation rather than adding another check framework.
