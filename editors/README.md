# Editor adapters and distributions

| Path | Responsibility |
| --- | --- |
| [vscode/vsix/](vscode/vsix/README.md) | VS Code adapter, language UX, extension manifest, and editor tests |
| [vscode/create_vsix.sh](vscode/create_vsix.sh) | Assemble the VSIX for local installation |
| [npm/](npm/) | CLI wrapper and native platform package manifests |
| [scripts/](scripts/) | Shared asset staging, package smoke tests, and release checks |
| [check_package_versions.js](check_package_versions.js) | Keep source Cargo/npm/VSIX versions aligned |

Installation and editor workflows are in [usage](../docs/usage.md). Use the
[release checklist](../docs/development/release.md) for package preparation and
native acceptance; it points to the same checks that CI runs.

Editor code owns presentation and commands. Ruby identity, inference, and
resolution belong to the server's analysis engine. Keep packaged assets and
licenses aligned through the shared staging scripts when adding a runtime input.
