---
name: release
description: "Prepare or publish a Ruby Fast LSP release with aligned package versions, candidate-specific validation, and the existing release workflow."
---

# Release preparation and publication

Read `docs/development/release.md` and `.github/workflows/release.yml`. Separate
candidate preparation, local installation, and publication according to the
user's requested scope; reuse authorization already given.

1. Inspect the current version, branch, diff, and requested release increment.
   Preserve unrelated changes. Resolve a missing version choice if it cannot be
   inferred from the request or release policy.
2. Keep the root Cargo version/lockfile, npm package versions and optional
   dependencies, and source VSIX package/lockfile versions aligned. The exact
   checked paths are in `editors/check_package_versions.js`; run that checker.
   Updating only Cargo.toml is insufficient.
3. Run the shared candidate gates and installed-artifact checks from the release
   guide. Retain commit/artifact identities, results, and untested combinations.
4. Prepare concise release notes for the final change. Review the intended commit
   and tag against the tested candidate. Do not treat historical reports as fresh
   acceptance or silently include unrelated work in the release.
5. Publish only within the user's authorization. Pushing `v<server-version>`
   triggers npm, Marketplace, Open VSX, and GitHub publication. When authorization
   is missing, finish preparing the concrete candidate before requesting it.

Server/npm versions use SemVer. CI derives the published editor CalVer; keep
source manifests on the aligned server version before packaging. Verify the
actual workflow result before reporting a release as published. Do not hardcode
an assumed remote, branch, tag, or successful publication.
