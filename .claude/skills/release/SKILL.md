---
name: release
description: "Release a new version of ruby-fast-lsp. Bumps version in Cargo.toml, commits, tags, and pushes to trigger CI. Use when the user says /release, 'release', 'publish', 'bump version', 'cut a release', or 'new version'."
---

# Release

Bump version, commit, tag, and push. CI handles the rest (npm, VSIX, GitHub Release).

## Process

1. Ask the user: **patch**, **minor**, or **major**? (or accept if already specified)
2. Read current version from `Cargo.toml` (line starting with `version =`)
3. Compute new version using SemVer rules
4. Update `Cargo.toml` with the new version
5. Show the user:
   - Current version → New version
   - What CI will publish: npm (`@ruby-fast/lsp`), VS Code Marketplace, Open VSX, GitHub Release
   - Ask for confirmation before proceeding
6. On confirmation:
   ```bash
   git add Cargo.toml
   git commit -m "release: v{NEW_VERSION}"
   git tag v{NEW_VERSION}
   git push origin main
   git push origin v{NEW_VERSION}
   ```
7. Show the GitHub Actions run URL so the user can monitor

## Rules

- **Only edit `Cargo.toml`** — npm and VSIX versions are synced by CI from the git tag
- Never push without explicit user confirmation
- If there are uncommitted changes besides Cargo.toml, warn the user and ask how to proceed
- Use `git tag` not `git tag -a` (lightweight tags)
