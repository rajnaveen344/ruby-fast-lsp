---
name: release
description: "Release a new ruby-fast-lsp version by bumping Cargo.toml, committing, tagging, and pushing after confirmation."
---

# Release

Use this skill when the user asks to release, publish, bump version, cut a release, or create a new version.

## Versioning

- `Cargo.toml`: SemVer, for example `0.2.7`.
- npm packages: synced by CI from the git tag.
- VS Code/Open VSX extension: CalVer computed by CI from the current date.

## Process

1. Ask for patch, minor, or major unless the user specified it.
2. Read the current package version from `Cargo.toml`.
3. Compute the new SemVer.
4. Update only `Cargo.toml`.
5. Show current version, new version, and what CI will publish.
6. Ask for explicit confirmation before commit/tag/push.
7. On confirmation:

```bash
git add Cargo.toml
git commit -m "release: v{NEW_VERSION}"
git tag v{NEW_VERSION}
git push origin main
git push origin v{NEW_VERSION}
```

## Rules

- Never push without explicit user confirmation.
- If unrelated uncommitted changes exist, warn the user and ask how to proceed.
- Use a lightweight tag (`git tag v...`), not an annotated tag.
