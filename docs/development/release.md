# Release checklist

Use this checklist for an identified candidate. It describes required evidence;
it is not a rolling claim that the current checkout or a platform has passed.
Record candidate-specific results with the release artifact or PR, including
anything failed or not run. Historical test counts and performance reports do
not certify a new binary.

## 1. Identify the candidate

Record commit, server version, extension version, artifact checksums, and chosen
platform/runtime combinations. Source package versions must agree with the root
Cargo version; `editors/check_package_versions.js` defines the checked manifests,
optional npm dependencies, and VSIX lockfile fields. Keep Cargo.lock aligned too.

The server/npm version uses SemVer. The release workflow computes the
published VS Code/Open VSX CalVer separately. Do not manually apply that published
CalVer to source manifests before their consistency check.

## 2. Review validation before choosing the release

Release readiness is a maintainer decision. Run or review the independent
`Validate` workflow before manually cutting a tag or dispatching publication.
The `Release` workflow does not invoke or wait for `Validate`; it starts with
native builds and retains native tests and installed-package checks. A release
run passing does not imply that the separate simulation campaigns ran.

Use the same runners as [.github/workflows/validate.yml](../../.github/workflows/validate.yml).
The CI file also specifies toolchain prerequisites: Rust, Node, Python, Java,
and Ruby for oracle execution. Install the editor's test dependencies first:

```sh
npm ci --prefix editors/vscode/vsix
node editors/scripts/release_checks.js correctness
node editors/scripts/release_checks.js simulation
```

`correctness` runs structure-policy checks, package-version consistency, workspace
Rust tests, editor/package tests, and explicit inference/precision reports.
`simulation` runs Ruby oracle controls, release-mode simulations, both explicit
synthetic campaigns through the `simulation` binary, and the deterministic performance budgets. Set
`SIM_REAL_CORPUS_ROOT` only for a deliberately selected read-only corpus; otherwise
that check remains recorded as not run.

Logs and machine-readable summaries go to `target/release-evidence/` (override
with `RUBY_FAST_LSP_EVIDENCE_DIR`). The runner rejects failures, every ignored Rust test, and missing or mismatched
simulation completion reports. Explicit campaigns are separate from the ordinary
workspace suite; an unselected real corpus is never counted as passed.

The separate CI fault-detection campaign proves that reviewed injected defects fail
their intended assertions:

```sh
python3 -B -m unittest discover -s support/simulation -p 'test_fault_campaign.py'
python3 -B support/simulation/run_fault_campaign.py --run
```

This campaign belongs to release/CI acceptance or simulator changes. It is not
required for every documentation edit. See [simulation](simulation.md) for replay
and for the distinction between consistency and independent semantic evidence.

## 3. Validate the installed artifacts

The [release workflow](../../.github/workflows/release.yml) builds and checks npm
and VSIX packages for macOS ARM64, macOS Intel, Linux x64 GNU, Linux ARM64 GNU,
and Windows x64. Linux ARM64 uses the native `ubuntu-24.04-arm` runner for both
builds and installed-package verification. Linux musl, 32-bit ARM, and Windows
ARM64 are not current package targets. A successful cross-compile is not a native
installation test.

Use the workflow's asset staging and package smoke scripts; do not hand-copy an
incomplete subset of stubs, framework extensions, licenses, or JRuby assets.
For a local VSIX and real VS Code acceptance:

```sh
./editors/vscode/create_vsix.sh --current-platform-only
node editors/scripts/smoke_vscode.js <candidate.vsix> <code-executable>
```

Record the exact artifact, OS/architecture, editor, Ruby, Bundler, JDK when
relevant, and the outcome of these user workflows:

- Cold indexing and navigation before all definitions have been opened.
- Unsaved cross-file edits during indexing; syntax failure and recovery;
  save/close/reopen; restart with open buffers.
- Multiple project roots and runtimes, dependency navigation, and missing-runtime
  behavior without state leaking across projects.
- Hover, hints, completion, rename previews, diagnostics, and Unicode ranges.
- Optional linting/formatting, ERB safeguards, framework activation, and test/view
  lenses with their required tools installed.

Keep native package checks, real-editor checks, and external feedback distinct.
Mark untested combinations as not run. See [usage](../usage.md) for troubleshooting,
rollback, and support boundaries.

## 4. Publish within the requested scope

Review the final diff and candidate-specific release notes. Tag-triggered
[release CI](../../.github/workflows/release.yml) publishes npm, Marketplace,
Open VSX, and the GitHub release after its checks. Pushing `v<server-version>`
therefore has publication side effects. Follow the user's existing authorization;
preparing or testing a candidate alone does not authorize publication.

Release notes should state user-visible changes, known limitations, and the
validated combinations for that artifact. Installation failures, incorrect
edits, stale results, isolation leaks, crashes, and unsupported concrete type
claims require triage before broader promotion. Intended Unknown outcomes are
separate from defects and missing validation.

For a weekly extension release, dispatch the `Release` workflow on the intended
commit with `publish_extensions=true`. It runs native builds, native tests,
and VSIX verification, then publishes Marketplace and Open VSX using
`ISO_YEAR.ISO_WEEK.PATCH`. Source manifests retain their aligned server version;
this dispatch does not publish npm packages or create a server-version tag.
The default manual dispatch (`publish_extensions=false`) builds, checks, and packages
without publishing. Review both registry jobs and verify the resulting versions
before reporting the extension as published.
