# Public beta guide

## Candidate status

The next broader public-release candidate is **unreleased**. This guide defines
the intended beta checks and known support boundaries; it does not certify a
build. Track the exact candidate commit, package versions, checksums, test
results, and outstanding defects in the [release-readiness checklist](release-readiness.md).
Historical measurements and successful compilation do not establish validation
of the candidate. Every unverified platform or runtime combination stays
**pending** until evidence for that exact candidate is recorded.

See the [draft candidate release notes](release-notes-next.md) for the changes
being prepared and the validation still required.

## Support matrix

The current [release workflow](../.github/workflows/release.yml) defines four
package targets. Native installation and editor validation are separate checks.

| Platform | Package target | Candidate validation |
| --- | --- | --- |
| macOS ARM64 (Apple Silicon) | `darwin-arm64` | Host validation pending |
| macOS Intel | `darwin-x64` | Native validation pending |
| Linux x64 with GNU libc | `linux-x64` | Native validation pending |
| Windows x64 | `win32-x64` | Native validation pending |
| Linux ARM64 | None | Not currently a published target |
| Linux with musl libc | None | Not currently a published target |
| Windows ARM64 | None | Not currently a published target |

Record OS versions, hardware, editor versions, and installation channels with
each result. Building a binary for another architecture is not a native test.

Runtime detection and signature compatibility are implementation capabilities,
not a tested Ruby support matrix:

| Runtime | Implemented behavior | Candidate validation |
| --- | --- | --- |
| MRI Ruby | Exact runtime selection and automatic project-marker detection; core stubs and runtime stdlib discovery | Exact versions and Bundler combinations pending |
| JRuby | Versioned compatibility and Java navigation support; recognized series are 9.0, 9.1, 9.2, 9.3, 9.4, 10.0, and 10.1 | Exact JRuby, JDK, and Bundler combinations pending |
| TruffleRuby | Runtime detection and Ruby-compatibility version parsing | Exact versions and Bundler combinations pending |
| No usable runtime | Conservative bundled Ruby 3.0 core-stub fallback; runtime-dependent stdlib discovery is skipped | Missing-runtime workflow pending |

The implementation sources are [runtime detection](../src/indexer/version/ruby_version.rs),
[JRuby series](../crates/jruby-support/src/version.rs), and
[stdlib/core indexing](../src/indexer/indexer_stdlib.rs). Recognizing an old
runtime identity does not promise complete language semantics or native
compatibility for that runtime. No full MRI or TruffleRuby version range is
claimed here before candidate testing.

Bundled framework packages require complete lockfile data in their owning
project. Their manifest activation ranges are:

| Package | Locked gem required for activation |
| --- | --- |
| [Rails](../extensions/rails-ruby/extension.toml) | `rails >= 6, < 9` |
| [RSpec](../extensions/rspec-ruby/extension.toml) | `rspec-core >= 3, < 4` |
| [Minitest](../extensions/minitest-ruby/extension.toml) | `minitest >= 5, < 7` |
| [Sinatra](../extensions/sinatra-rust/extension.toml) | `sinatra >= 2, < 5` |
| [Cucumber](../extensions/cucumber-rust/extension.toml) | `cucumber >= 9, < 12` |

These ranges control static extension activation. They do not certify every
framework release, its required Ruby version, or execution of the application's
tests. Candidate activation and editor checks remain pending for each chosen
framework/runtime combination.

## Install, troubleshoot, and roll back

Before changing versions, record the working server and editor-extension
versions and installation channel. The server's npm version and the VS Code
extension's version may differ. Obtain a candidate only from a maintainer's
identified build; this guide does not imply one has been published.

For npm, replace the placeholder with an available, explicitly selected version:

```sh
npm install -g @ruby-fast/lsp@<candidate-version>
ruby-fast-lsp --version
```

Configure stdio clients as described in [Setup](../README.md#setup). For a VSIX
candidate, use VS Code's **Extensions: Install from VSIX...**, select the supplied
file for your platform, and run **Developer: Reload Window**. Installing the npm
server does not replace the binary bundled in the VS Code extension.

To roll back an npm installation:

```sh
npm install -g @ruby-fast/lsp@<previous-version>
ruby-fast-lsp --version
```

For VS Code, install the saved previous VSIX or choose **Install Another
Version...** on the extension when that channel provides the previous version,
then reload the window. Restart other LSP clients after changing their server
binary. Record whether rollback restores the expected behavior.

For runtime issues, click the bottom-right Ruby runtime item or run **Ruby Fast
LSP: Select Runtime**. Check the active project's exact runtime with **Ruby Fast
LSP: Show Runtime Status**. Auto resolves project markers without substituting
a nearby installed version. **Save Runtime to .ruby-version** is a separate,
confirmed project write; ordinary runtime selections stay private to the VS
Code workspace. See [VS Code setup](../README.md#vs-code).

For diagnostics or stalled indexing, open the **Ruby Fast LSP** output channel
and **Ruby Fast LSP: Show Project Indexing Status**. Temporarily set
`rubyFastLsp.logLevel` to `debug` for a reproduction, then return it to `info`.
`trace` is very verbose and should only be used for a narrowly scoped request.
These are the supported values in the [VS Code manifest](../editors/vscode/vsix/package.json).
Review and sanitize logs before sharing them. A missing runtime, an unresolved
dependency, and an unsupported inference form require different fixes; include
the exact diagnostic text and whether indexing had finished.

When using other Ruby extensions, choose one provider for each overlapping
feature, especially diagnostics, formatting, completion, and test lenses.
Disable duplicate providers for the beta reproduction or use a separate editor
profile. A debugger extension can still supply the `rdbg` integration required
by Debug lenses. State which providers remained enabled in a report.

## Intentional boundaries

- **Ruby inference:** incomplete or ambiguous evidence produces `Unknown`.
  Runtime reflection, string `eval`, unsupported mutation or escape, and
  exceeded inference bounds must not produce a guessed concrete type. Hash
  shapes model Hash-backed values, not arbitrary Ruby object properties.
  See [structural Hash limits](structural-hash-shapes.md),
  [higher-order call boundaries](higher-order-call-inference.md), and
  [callable-body boundaries](callable-body-inference.md).
- **Dependencies and native APIs:** Bundler inputs belong to the exact owning
  project. Dependency files support navigation and declared types but do not
  become editable project sources. Binary-only native APIs need Ruby, RBI,
  RBS, or an extension package; the server cannot reconstruct them from the
  binary alone. Gem/stdlib methods without a declared return may remain
  `Unknown`. See [project indexing](../README.md#project-indexing).
- **RBS:** supported declarations provide signatures and type overlays, with
  navigation preferring matching Ruby implementations. RBS files are not Ruby
  diagnostic, rename, or project workspace-symbol sources. Interfaces, type
  aliases, and method aliases are parsed but do not yet become navigation
  declarations. See [native and generated declarations](../README.md#project-indexing).
- **Rails:** conventional static associations, callbacks, routes, jobs,
  concerns, and controller view lenses have bounded coverage. Dynamic
  association options, full inflection, concern dependency declarations, and
  advanced routes are not fully modeled. Route helpers currently follow
  controller inheritance, not view-template contexts. See the
  [Rails extension contract](../extensions/rails-ruby/README.md).
- **ERB:** Ruby features use complete Ruby tag bodies with original source
  positions. Comments, escaped tags, host text, and unclosed tags are masked
  from Ruby analysis. VS Code provides complementary HTML features, but HTML
  diagnostics, formatting, links, colors, and rename are not delegated.
  Ruby formatters and linters do not edit templates. See
  [ERB templates](../README.md#erb-templates).

An expected `Unknown` is not itself a release blocker. A concrete claim without
enough evidence, a stale result after an edit, or a destructive edit is a defect.
Record missing validation separately from these intentional boundaries.

## Beta acceptance scenarios

Run these against the installed candidate, starting with a small neutral
project. Record each scenario as **passed**, **failed**, or **not run**, together
with versions and relevant evidence. A scenario that is not run remains pending.

| Scenario | Checks |
| --- | --- |
| Start and cold indexing | Open a fresh workspace; indexing reaches Ready; navigation, hover, completion, diagnostics, and symbols work without opening every definition file. |
| Edit, save, close, and reopen | Change a cross-file return type or method name, introduce then repair syntax errors, and reopen files. Results follow current content; old diagnostics and references disappear. Repeat with unsaved edits during startup. |
| Restart | Reload the editor window with open buffers; indexing and runtime status recover; features agree before and after restart, including repeated restarts. |
| Dependency changes | In a disposable fixture, install or change a locked dependency and add/remove a static require. Check navigation and diagnostics after the supported refresh/restart path; record whether restart was necessary. |
| Multiple projects | Open projects with different Gemfiles and runtimes, including duplicate class names. Navigation, diagnostics, dependencies, and runtime status remain attached to the owning project. Add/remove a workspace folder while documents are open. |
| Missing runtime | Open a project whose exact selected runtime is unavailable. Core constants remain usable, no nearby runtime is silently selected, and missing runtime-dependent inputs are visible in status/logs. |
| Linting and formatting | Use **Ruby Fast LSP: Select Linter** and **Ruby Fast LSP: Select Formatter** independently. Check unsaved Ruby buffers, no-change output, a failing tool, and ERB safeguards; edits must be correct and reversible. |
| Test and view lenses | Run a neutral RSpec/Minitest test and check the exact target. Exercise Debug only with the required debugger installed. Check a conventional Rails Open View target and a missing template. Record unmet prerequisites as not run. |
| Semantic and simulation regressions | Run the release checklist's required acceptance and deterministic simulation checks. Retain the seed and replay artifact for any failure; report large-scale or real-corpus scenarios separately when not exercised. |
| Longer editing session | Repeat edits, queries, save/reopen, and restart on a representative project. Record readiness and query timings, CPU, memory, crashes, and growing resource use; compare against the candidate's documented performance checks. |

Check definition/reference destinations and rename previews, not only whether
a command returned a response. Include non-ASCII text in the neutral fixture
when a report involves incorrect ranges. See
[test discovery prerequisites](../README.md#test-discovery-and-execution) for
test runner and debugger requirements.

## Feedback and triage

Use the [bug report form](../.github/ISSUE_TEMPLATE/bug_report.yml). Include the
candidate and prior working versions, installation channel, OS/architecture,
Ruby implementation and exact version, Bundler, editor, enabled Ruby extensions,
workspace layout, lifecycle steps, expected/actual behavior, and sanitized logs.
For simulation failures, include the seed, replay command, and a sanitized
failure artifact. Proprietary source is never required: a reduced, neutral
example with the same semantic shape is preferred.

Release-blocking reports include:

- Installation/startup failure on a target proposed as validated, missing
  packaged assets, or a package/version mismatch.
- Crashes, deadlocks, failure to reach readiness, or sustained unbounded
  resource growth in a reproducible supported workflow.
- Incorrect edits or rename targets, lost unsaved content, project-isolation
  leaks, or stale indexing results overwriting newer edits.
- Unsupported concrete type claims, missing valid navigation targets, or
  reproducible false diagnostics in a supported, fully indexed case.
- A regression against required semantic, simulation, package, or performance
  gates, including failures that occur only after a particular edit ordering.

Track unsupported-feature requests separately, with a link to the relevant
boundary. A maintainer must reproduce and classify a report before closing it
as intentional. Broader promotion requires resolved blocking reports and
recorded native validation for the advertised combinations; untested
combinations remain pending in the release checklist.
