# Ruby Fast LSP: 9/10 Product Goal

## Reusable Goal Text

Elevate Ruby Fast LSP from its current estimated 6.5/10 completeness to a 9/10 direct competitor to Shopify Ruby LSP. Preserve Ruby Fast LSP's advantages in semantic analysis, method navigation, references, RBS/YARD inference, deterministic simulation, Rust performance, agent-first APIs, and sandboxed extensions while closing the everyday editor, Rails, template, packaging, and production-confidence gaps. Complete the versioned, deterministic, permission-bounded, and failure-isolated extension architecture before broad framework expansion; RSpec must use its public contracts as the reference implementation, and Rails and third-party extensions must use those same contracts without privileged access to engine stores or LSP server state. Work incrementally with strict TDD, keep the simulator as the primary comprehensive semantic verification system, run the complete local pre-push verification gate before every commit or push, and update this document whenever scope or measured readiness changes.

## Product Definition

A 9/10 Ruby Fast LSP means:

> A Ruby developer can replace Shopify Ruby LSP with Ruby Fast LSP and rarely
> miss an important workflow, while receiving meaningfully better semantic
> navigation, diagnostics, responsiveness, and agent-oriented code intelligence.

The goal is not exact feature parity. Features should be implemented when they
contribute materially to the replacement experience or strengthen Ruby Fast
LSP's differentiation.

## Baseline

Estimated state as of July 2026: **6.5/10 overall**.

| Area                    | Current estimate | Current state                                                                     |
| ----------------------- | ---------------: | --------------------------------------------------------------------------------- |
| Semantic architecture   |              8.5 | Reusable core, engine, indexer, and inference boundaries are established.         |
| Definition/navigation   |              8.0 | Strong method, constant, mixin, visibility, hierarchy, and dynamic-form coverage. |
| References              |              8.0 | Method, constant, and local reference support with centralized resolution.        |
| Type inference          |              7.5 | RBS/YARD, generic substitution, local flow tracking, and return propagation.      |
| Diagnostics             |              7.0 | Ambitious semantic diagnostics; real-world compatibility still needs hardening.   |
| LSP feature breadth     |              7.0 | Agent-critical features exist; several everyday editor features are missing.      |
| Rails/framework support |              4.5 | Initial DSL handling and RSpec extension, but not a complete Rails experience.    |
| Formatting/refactoring  |              3.5 | Far behind mature formatter, linter, quick-fix, and refactoring integration.      |
| Editor/package polish   |              5.5 | npm and VSIX work, but packaging and version consistency need hardening.          |
| Production evidence     |              6.0 | Strong simulator and tests; limited packaged-editor and long-term user evidence.  |

The semantic foundation is the strongest part of the project. The primary gap
is product completeness rather than a missing analysis architecture.

## Existing Advantages to Preserve

- Cross-file method and constant resolution through inheritance and mixins.
- Method, constant, and local references.
- Call hierarchy, type hierarchy, and implementation lookup.
- RBS/YARD-backed inference and generic substitution.
- Flow-sensitive local type tracking.
- Semantic diagnostics such as unresolved method and constant detection.
- A deterministic project simulator checked against a separate oracle.
- A reusable analysis engine that does not depend on LSP protocol types.
- Rust-native performance potential.
- Agent-first navigation and debugging APIs.
- Sandboxed Wasm extensions and semantic patches.

Do not weaken semantic accuracy merely to copy a competing feature. Do not move
reusable semantics back into LSP adapters.

## Competitive Gaps

Shopify Ruby LSP currently has a broader complete-product experience, including:

- Signature help.
- Document highlights and selection ranges.
- Formatter and linter integrations.
- Quick fixes and refactoring code actions.
- ERB support and host-language request delegation.
- Test Explorer, run, and debug workflows.
- Mature Bundler, Ruby environment, and index configuration behavior.
- A larger add-on ecosystem.
- Automatic Rails integration and optional runtime introspection.
- Rails routes, associations, validations, callbacks, generators, and
  controller/view navigation.
- Greater release history and real-world operational hardening.

Official references:

- <https://shopify.github.io/ruby-lsp/>
- <https://shopify.github.io/ruby-lsp/design-and-roadmap.html>
- <https://shopify.github.io/ruby-lsp/add-ons.html>
- <https://shopify.github.io/ruby-lsp/rails-add-on.html>

Re-check these sources before each major planning cycle because the competing
feature set will change.

## Milestone 1: Everyday LSP Completeness

Target rating: **7.3/10**.

Priority order:

1. Done: signature help using engine method facts, cross-file YARD metadata,
   and RBS overload signatures, including visibility/MRO resolution, nested
   calls, variadic and keyword selection, and edit/reindex lifecycle coverage.
2. Done: opt-in RuboCop and Standard diagnostics integration on document open
   and save, using current-buffer stdin, workspace-aware configuration,
   structured command argv, JSON diagnostics with UTF-16 position conversion,
   timeout/failure isolation, and VS Code settings.
3. Done: preferred quick-fix code actions for safe RuboCop and Standard fixes,
   operating on the current unsaved buffer and returning an editor-applied
   full-document edit only when safe corrected output differs.
4. Done: cross-file constant, class, and module rename with prepare-rename
   support, exact declaration token ranges, namespace-aware engine resolution,
   collision and external-source safety, UTF-16 positions, and edit/reindex
   lifecycle coverage.
5. Done: document highlights for constants, methods, and locals, composed from
   centralized semantic references, filtered to the current document, and
   covered through edit/reindex lifecycle tests.
6. Done: syntax-aware selection ranges from identifier/message tokens through
   expressions, statements, and enclosing scopes, with multi-position,
   malformed-buffer, UTF-16, and edit lifecycle coverage.
7. Done: workspace-relative index include/exclude patterns and explicit gem
   include/exclude configuration, with exclusion precedence, nonstandard source
   support, deterministic discovery, invalid-glob errors, transitive dependency
   filtering, and VS Code restart behavior that prevents stale facts.
8. Done: opt-in full-document RuboCop and Standard formatting using the current
   unsaved buffer, safe correction modes, structured command argv, UTF-16 edit
   ranges, failure isolation, lifecycle coverage, and packaged VS Code settings.

Completion criteria:

- Each feature is advertised accurately through server capabilities.
- Each feature has integration coverage through `check()` or `FakeEditor`.
- Lifecycle-sensitive features survive open, change, save, and reindex.
- External formatter/linter failures are reported clearly and do not corrupt
  documents or engine state.
- No new semantic policy is duplicated outside `ruby-analysis::engine`.

## Milestone 2: Production Extension Platform

Target rating: **7.7/10**.

Complete the extension architecture before expanding framework-specific support.
RSpec remains the reference implementation, and Rails must be implementable
through the same public contracts without privileged core hooks.

Implement and stabilize:

1. A versioned extension ABI with explicit server compatibility requirements.
2. Manifest validation for identity, runtime, capabilities, permissions,
   checksums, and supported server versions.
3. Domain-level extension inputs and outputs using facts, semantic patches,
   ranges, Ruby names, and types rather than LSP protocol types.
4. Deterministic extension discovery, activation order, fact merging, patch
   application, and conflict handling.
5. Extension lifecycle support for discovery, activation, status reporting,
   reload where safe, deactivation, and clean shutdown.
6. Failure isolation so an invalid, incompatible, slow, or crashing extension
   cannot corrupt engine state or take down unrelated editor features.
7. Enforced process and Wasm permissions, checksums, fuel limits, memory limits,
   input/output size limits, and actionable error reporting.
8. Bundled, project-local, and explicitly configured extension discovery with a
   clear precedence and trust policy.
9. A reusable extension SDK, authoring documentation, package template, and
   black-box extension test harness.
10. Stable hooks for declaration DSLs, document symbols, code lenses, semantic
    facts, watched files, and optional runtime-backed knowledge.

Architecture requirements:

- Extensions provide external DSL and library knowledge; they are never the
  global source of semantic truth.
- The engine validates, ingests, resolves, and owns the final graph, symbol,
  reference, type, and diagnostic state.
- Extensions must not mutate engine stores, document state, or LSP server state
  directly.
- Extension contracts must depend on reusable `ruby-analysis` domain types, not
  `tower_lsp` types, editor commands, or client-specific response shapes.
- Semantic patch application must be deterministic and independently testable.
- Conflicting extension output must follow an explicit policy; it must not
  depend on hash-map order, filesystem order, or timing.
- Framework-specific behavior must not be added to the core when it can be
  represented through a general extension hook.
- Runtime introspection is an optional extension input. Static indexing must
  remain useful when the runtime process is unavailable.

Planned extension architecture:

1. `crates/extension-api` is the versioned, serializable guest contract. It
   exposes domain events, ranges, Ruby names/types, semantic patches, response
   patches, command requests, capabilities, and permissions; it must not expose
   engine stores, server objects, or LSP protocol types.
2. `src/extensions` is the untrusted-guest boundary and runtime host. It owns
   discovery, manifests, compatibility and checksum checks, activation,
   lifecycle events, settings and watched-file delivery, process brokering,
   resource limits, provenance validation, deterministic ordering, conflict
   resolution, failure isolation, and observable status.
3. Extension semantic output is converted into ordinary per-file analysis
   facts before `AnalysisEngine::replace_facts`. Extensions never write engine
   stores directly, and stale extension facts disappear through the same
   reindex lifecycle as parser-produced facts.
4. `ruby-analysis::engine` remains the sole owner of final symbol, method,
   reference, graph, type, and diagnostic truth. Normal engine resolution and
   query APIs must treat accepted extension facts exactly like equivalent
   parser-produced facts.
5. The stable semantic patch vocabulary must cover generated namespaces,
   methods and parameters, typed constants, mixins/inheritance, references,
   diagnostics, and the declaration relationships needed by Rails and other
   DSL-heavy libraries. New patch families require validation, deterministic
   conflict identity, provenance, lifecycle removal, and black-box query tests.
6. RSpec is the bundled reference extension, while a separately packaged
   example extension proves that an external author can use the SDK, build a
   Wasm artifact, pass validation, and contribute semantic/editor behavior
   without modifying the server. Rails must use only these same public paths.
7. Optional runtime knowledge enters through permission-bounded process
   requests and versioned events. Runtime output becomes validated patches; it
   cannot bypass static indexing, mutate server state, or make core Ruby
   analysis depend on a running application.

Implementation order:

1. Finish the general semantic patch vocabulary and its deterministic merge
   rules.
2. Prove every patch family through SDK serialization, invalid-input tests,
   actual-Wasm black-box tests, edit/reindex removal, and simulator coverage
   where it exercises reusable engine semantics.
3. Complete lifecycle/reload and packaged-extension smoke coverage.
4. Build Rails support exclusively on the stabilized public contracts.

Completion criteria:

- The bundled RSpec extension uses only supported public extension contracts.
- A minimal third-party example extension can add a DSL declaration, document
  symbol, code lens, and semantic patch without modifying the server.
- Compatibility mismatch, checksum failure, permission denial, fuel exhaustion,
  memory exhaustion, malformed output, and extension crash paths are tested.
- Reindex and edit lifecycle tests prove that stale extension facts are removed
  and replaced through the normal engine write path.
- Loading the same extensions in the same configuration produces identical
  semantic state and query results.
- Extension status and failures are observable without polluting ordinary Ruby
  diagnostics.
- The packaged VSIX discovers and loads its bundled RSpec extension.
- The public extension documentation is sufficient to build an extension
  without reading server internals.

### Extension architecture continuity

Milestone 2 is an established foundation, not a future rewrite target. Future
work must preserve and extend these public seams:

- `crates/extension-api` owns the versioned guest contract and domain patch
  vocabulary.
- `src/extensions` owns the untrusted Wasm host, lifecycle, discovery, trust,
  permissions, resource limits, provenance checks, deterministic conflicts,
  process brokering, and status reporting.
- Extension output enters analysis only as validated per-file facts through the
  normal engine replacement lifecycle; extensions never mutate engine or LSP
  state directly.
- RSpec, Minitest, Rails, and the example DSL package must remain ordinary
  consumers of the same public contracts. Framework-specific privileged hooks
  are prohibited.

Remaining extension-platform work toward 9/10 is evolutionary:

1. Add a new semantic patch or event only when a real framework workflow cannot
   be expressed by the existing vocabulary, and keep it framework-neutral.
2. For every new contract, require serialization compatibility, validation,
   deterministic conflict identity, provenance, failure isolation, edit/reindex
   removal, and actual-Wasm black-box coverage.
3. Keep SDK and authoring documentation synchronized with the implemented ABI,
   and prove third-party usability without importing server internals.
4. Preserve packaged-artifact smoke coverage for every bundled extension and
   verify that developer paths cannot mask missing packaged assets.
5. Treat runtime introspection as optional, trusted, permission-bounded input;
   static analysis must remain useful and deterministic without a running app.

Do not replace this architecture with direct framework logic in the indexer,
public engine-store access, LSP-shaped guest contracts, or a second semantic
write path. Any proposed change to these boundaries requires an explicit
architecture rationale and regression evidence.

## Milestone 3: Credible Rails Development

Target rating: **8.1/10**.

Build a first-class Rails extension supporting the highest-value workflows:

1. Done: Active Record associations.
2. Done: validations and callbacks.
3. Done: route and URL helpers.
4. Done: route-to-controller and conventional controller-to-view navigation.
5. Done for ordinary composition: Active Support concern instance and
   `class_methods` navigation; dependency declarations remain an enhancement.
6. Done: Active Job enqueue entry points.
7. Done: Minitest and RSpec discovery.
8. Done: test code lenses and run/debug commands.

Architecture requirements:

- Rails must use the public extension platform and must not receive privileged
  access to engine stores or LSP server state.
- Framework DSL knowledge must live in extensions or explicit semantic patches.
- Avoid growing a permanent list of framework-specific names in the core
  indexer.
- Static facts remain the default.
- Runtime introspection must be optional, bounded, observable, and isolated from
  core indexing failures.
- The engine remains the final owner of graph and symbol truth.

Completion criteria:

- Representative Rails fixtures cover navigation, references, hover, symbols,
  and diagnostics.
- Simulator shapes cover the reusable semantic mechanisms behind Rails DSLs.
- A manual packaged-editor smoke test succeeds on at least one representative
  Rails application.

## Milestone 4: Templates and Ruby Project Coverage

Target rating: **8.6/10**.

Implement:

1. Done: ERB parsing and stable source-range mapping.
2. Done for the current Ruby feature surface: Ruby LSP features inside ERB
   regions; HTML delegation is owned separately by VS Code.
3. Done for range-safe read/query features: HTML completion, hover, symbols,
   folding, selection ranges, and highlights in VS Code. Whole-document HTML
   formatting/diagnostics and edit-producing features remain intentionally out.
4. Done: `.rake`, `.gemspec`, Thor, and common Ruby extension/filename
   handling across discovery, VS Code association, watchers, and packaging.
5. Done: explicit generated, vendored, dependency, included/excluded, trust,
   workspace-query, rename, diagnostics, and watched-file lifecycle policy.
6. Done: Ruby/RBI declarations, project RBS signature ingestion and overlay
   precedence, and public extension patches form the explicit strategy for
   native-extension and generated API declarations.

Completion criteria:

- LSP positions remain correct with multibyte text and embedded-language
  boundaries.
- Ruby edits and diagnostics never target host-language ranges incorrectly.
- Template support is covered by black-box editor tests where client delegation
  is involved.

## Milestone 5: Measured Production Confidence

Target rating: **9.0/10**.

The simulator remains the primary comprehensive semantic test system. Real
projects are a small release smoke corpus, not a redundant second semantic test
suite.

Required evidence:

- Zero known required simulator coverage buckets missing.
- All root and workspace tests pass before push.
- No known crashes on the selected release smoke projects.
- Cold indexing, edit latency, navigation latency, and memory have recorded
  budgets.
- Completion, hover, definition, references, and diagnostics meet defined p95
  latency targets.
- False-positive semantic diagnostics remain within an explicit reviewed budget.
- Editing does not trigger broad project-wide affected-file work in the typing
  critical path.
- Semantic export fingerprints distinguish body-only edits from exported-symbol
  changes.
- Diagnostic refresh is bounded for visible/open files during typing.
- The installed npm binary and packaged VSIX are tested, not only source builds.

## Local Pre-Push Gate

This project does not require hosted CI. Before every push or release candidate,
run the equivalent of:

```bash
cargo fmt --all -- --check
cargo test
cargo test --workspace --exclude ruby-fast-lsp
cargo build --release
./editors/vscode/create_vsix.sh --current-platform-only
```

Also perform focused tests during TDD. Do not wait until the full gate to discover
that a new regression test never failed or that the fix affects unrelated method
resolution.

Clippy should become part of the gate only after the existing lint backlog is
resolved and the intended lint policy is explicit.

## Packaging Completion

- Done: keep Cargo, npm package, npm platform packages, and VSIX versions
  consistent through an executable pre-package check.
- Done: ensure packaging output names match the version actually embedded in the
  artifact.
- Done: resolve shipped npm dependency vulnerabilities; the current production
  dependency audit reports zero findings.
- Done: verify the server starts through a freshly packed and installed npm
  wrapper over stdio with a real LSP initialize handshake.
- Done: verify the packaged VSIX contains its current-platform binary, stubs,
  safe dependency versions, and bundled RSpec extension.
- Add distribution targets only when they correspond to intended supported
  platforms; Linux ARM64 is the first likely addition.

## Product Priorities

When choosing the next task, prefer work in this order:

1. Correctness regression or crash.
2. Typing-path latency regression.
3. Packaging or startup failure.
4. Missing everyday replacement workflow.
5. Extension-platform blocker needed by multiple frameworks or integrations.
6. Rails ecosystem workflow.
7. Semantic accuracy improvement.
8. Optional polish or low-frequency feature.

Avoid starting a new capability while a current capability has known correctness
regressions or the local release gate is failing.

## Rating Rules

Do not raise the overall score merely because code exists. A capability counts as
complete only when it is:

- Correct for its declared scope.
- Covered by focused tests.
- Covered by lifecycle tests when state changes matter.
- Exposed accurately through LSP capabilities.
- Verified in a release build or packaged editor when integration matters.
- Documented for users when configuration or limitations matter.

Suggested weighting for future reviews:

| Category                            | Weight |
| ----------------------------------- | -----: |
| Semantic correctness and navigation |    25% |
| Everyday LSP feature breadth        |    20% |
| Rails and Ruby ecosystem support    |    15% |
| Performance and memory evidence     |    15% |
| Reliability and lifecycle behavior  |    10% |
| Editor and packaging experience     |    10% |
| Extensibility and agent experience  |     5% |

## Definition of Done

The 9/10 goal is achieved when:

1. The major Shopify replacement workflows listed above are present or have an
   intentionally documented alternative.
2. Rails developers can navigate and run ordinary application/test workflows
   without maintaining a second Ruby language extension.
3. Ruby Fast LSP demonstrates a semantic advantage in method navigation,
   references, diagnostics, or inference.
4. Performance and memory claims are supported by repeatable measurements.
5. Local release verification passes from a clean checkout.
6. npm and VSIX artifacts install and run with consistent versions.
7. RSpec, Rails, and third-party integrations can use a stable, documented,
   deterministic, and failure-isolated extension platform without modifying the
   semantic core.
8. Remaining gaps are narrow enhancements rather than missing daily workflows.

## Session Handoff Template

Use this at the beginning of a future goal session:

```text
Read AGENTS.md and goal.md completely. The active objective is to elevate Ruby
Fast LSP from the documented baseline toward 9/10 direct competitiveness with
Shopify Ruby LSP. Inspect the current repository and recent commits before
choosing work. Select the highest-priority incomplete item from goal.md that is
safe to execute now. Follow strict TDD for broken or missing behavior: establish
red, implement the minimum correct change, then run focused and broader tests.
Preserve the ruby-analysis core/engine/indexer/inference boundaries, keep method
resolution policy single-sourced in the engine, and use the simulator as the
primary comprehensive semantic verification system. Complete the production
extension platform described in Milestone 2 before broad framework expansion:
extensions must use versioned public domain contracts, produce deterministic
facts and semantic patches owned by the engine, and remain permission-bounded
and failure-isolated. RSpec is the reference implementation, and Rails must use
the same public extension contracts without privileged semantic-core hooks.
Before committing or pushing, run the local pre-push gate. Update goal.md with
completed work, measured evidence, rating changes, and newly discovered
blockers. Do not inflate the rating for code that is not tested, integrated,
and usable.
```

At the end of every goal session, record:

- What became user-visible.
- Tests and measurements that prove it.
- Any regression or compatibility risk discovered.
- The next highest-priority incomplete item.
- Whether the category ratings or overall rating genuinely changed.

## Progress Evidence

### July 2026: External lint diagnostics

- User-visible: RuboCop and Standard offenses are merged with Ruby Fast LSP
  diagnostics on document open and save; the external process never runs during
  `didChange`.
- Configuration: `rubyFastLsp.linter` selects `none`, `rubocop`, or `standard`;
  `rubyFastLsp.linterCommand` optionally supplies a structured argv such as
  `["bin/rubocop"]`.
- Correctness: RuboCop parser byte columns are converted to LSP UTF-16 positions,
  including multibyte source lines.
- Isolation: missing commands, abnormal exits, malformed JSON, and ten-second
  timeouts are logged without discarding syntax or semantic diagnostics.
- Focused evidence: runner/parser tests cover stdin, argv, working directory,
  exit status 1, Standard attribution, malformed output, timeout, and UTF-16
  conversion; lifecycle tests cover open/change/save and semantic-diagnostic
  preservation on linter failure.
- Next priority: safe RuboCop/Standard code actions using the correctability
  metadata already attached to diagnostics. Done in the follow-up commit;
  cross-file constant/class/module rename is now the next priority.
- Packaging risk unchanged: the local gate still emits a `0.2.3` VSIX while
  Cargo is `0.2.6`, and npm audit reports one moderate and one high dependency
  vulnerability. These remain explicit Packaging Completion blockers.
- Rating: with signature help, external lint diagnostics, and safe quick fixes
  integrated and package-verified, the current estimate is **6.7/10**. This is
  a modest breadth/refactoring increase; the 7.3 Milestone 1 target remains
  gated by rename, highlights, selection ranges, indexing configuration, and
  full-document formatting.

### July 2026: Safe linter quick fixes

- User-visible: correctable RuboCop and Standard diagnostics expose a preferred
  `quickfix` action and apply the corrected unsaved buffer through a
  `WorkspaceEdit`.
- Safety: RuboCop uses `--autocorrect` rather than unsafe `--autocorrect-all`;
  Standard uses its safe `--fix` mode. Noncorrectable diagnostics do not offer
  actions, unchanged output produces no edit, and failed correction processes
  produce no edit.
- Integration evidence: initialization advertises only `quickfix`; a real
  FakeEditor lifecycle test requests the action, applies its edit through
  `didChange`, and verifies resulting content. Runner tests verify Standard's
  safe flag and current-buffer stdin behavior.
- Architecture: subprocess execution and LSP edit shaping remain in `src/`;
  no external-linter policy or protocol types entered `ruby-analysis`.

### July 2026: Cross-file constant rename

- User-visible: classes, modules, and value constants can be renamed from a
  declaration or reference across project files; reopened definitions are all
  updated, and clients can probe exact ranges through `prepareRename`.
- Safety: invalid Ruby constant names, sibling collisions, and targets defined
  only in gems, stdlib, or stubs are rejected. Qualified references edit only
  the resolved terminal name rather than deleting their namespace prefix.
- Correctness: symbol facts retain exact declaration-name ranges, analysis and
  document position conversion uses LSP UTF-16 code units, and non-BMP text is
  covered by lifecycle tests.
- Architecture: constant identity, collision policy, project-source filtering,
  and deterministic edit ranges live in `ruby-analysis::engine`; Prism-derived
  declaration ranges live in the indexer; `src/capabilities/rename.rs` remains
  a protocol adapter while preserving the existing local-variable visitor.
- Focused evidence: 35 rename integration tests cover prepare capability,
  class/module/value-constant edits, cross-file and reopened definitions,
  namespace isolation, invalid/colliding names, Unicode, reindex lifecycle, and
  all existing local-variable cases. The `ruby-analysis` suite has 330 passing
  tests including exact name ranges, UTF-16 conversion, and external-source
  rejection.
- Next priority: document highlights, followed by selection ranges.
- Rating: current estimate is **6.8/10**. Rename closes an important daily
  refactoring gap, but Milestone 1 remains incomplete until selection ranges,
  indexing configuration, and formatting are shipped.

### July 2026: Document highlights

- User-visible: editors can highlight same-document occurrences of constants,
  methods, and local variables at the cursor.
- Architecture: the capability is a current-document projection of the existing
  semantic references query, preserving engine-owned constant/method identity,
  MRO, visibility, ambiguity, and local-scope behavior.
- Integration evidence: initialization advertises `documentHighlight`; tests
  cover same-document filtering against a cross-file occurrence, class
  constants, methods, locals, and refresh after `didChange` reindexing.
- Next priority: selection ranges.
- Rating remains **6.8/10**. Highlights close a useful editor-polish gap but are
  too small alone to justify another overall score increase.

### July 2026: Selection ranges

- User-visible: expand/shrink selection follows Ruby syntax from call messages
  and identifier tokens through chained expressions, assignments, statements,
  methods, and the file scope.
- Correctness: one nested response is returned for each requested position in
  input order; parent ranges strictly contain children. Empty and malformed
  buffers return valid fallbacks, and non-BMP text uses LSP UTF-16 positions.
- Architecture: generic Prism branch/leaf traversal and token refinement live
  in `ruby-analysis::indexer`; the server adapter only converts domain
  `TextRange` chains to nested LSP `SelectionRange` values.
- Integration evidence: initialization advertises `selectionRange`; focused
  tests cover chained calls, multiple positions, strict nesting, malformed and
  empty documents, emoji, and current-buffer refresh after `didChange`.
- Next priority: index include/exclude and dependency configuration.
- Rating remains **6.8/10**. Selection ranges complete another editor workflow,
  but indexing configuration and full-document formatting still gate Milestone
  1 completeness.

### July 2026: Configurable project and dependency indexing

- User-visible: `rubyFastLsp.indexing` exposes `includedPatterns`,
  `excludedPatterns`, `includedGems`, and `excludedGems` in VS Code and LSP
  initialization options.
- Source policy: standard Ruby files remain included by default; workspace-
  relative included globs can add nonstandard files such as `bin/console`,
  exclusions always win, `.git` is never traversed, and results are sorted for
  deterministic indexing.
- Dependency policy: explicitly included gems augment source-inferred roots;
  excluded gems win over both direct roots and transitive dependencies.
- Lifecycle safety: changing the setting in VS Code restarts the language
  server, rebuilding semantic state instead of retaining stale facts for newly
  excluded files. Other clients are documented to restart after changes.
- Failure behavior: invalid glob syntax aborts workspace indexing with the
  setting name and offending pattern instead of silently applying a partial
  configuration.
- Focused evidence: configuration round-trip, glob precedence, `.git`, invalid
  glob, dependency-scan reuse, explicit gem selection, and transitive exclusion
  tests pass; the 55-test release simulator remains green.
- Next priority: full-document formatting through the existing external-tool
  integration direction.
- Rating: current estimate is **6.9/10**. This closes a meaningful project-
  scale replacement workflow, but Milestone 1 and its 7.3 target still require
  full-document formatting and complete packaged verification.

### July 2026: Safe full-document formatting

- User-visible: the server advertises full-document formatting and VS Code
  exposes independent `rubyFastLsp.formatter` and `formatterCommand` settings
  for RuboCop or Standard.
- Safety: RuboCop uses `--autocorrect`, never `--autocorrect-all`; Standard uses
  `--fix`. The formatter runs only on an explicit formatting request, consumes
  the current unsaved buffer over stdin, and runs in the workspace root.
- Correctness: changed output becomes one full-document edit whose end position
  uses LSP UTF-16 units. Unchanged output returns no edit, and applying the edit
  through the editor lifecycle updates the analysis document through the normal
  `didChange` path.
- Isolation: disabled formatters, startup failures, timeouts, abnormal exits,
  non-UTF-8 output, and empty output for non-empty source return no edit and
  cannot mutate document or engine state. Interactive clients receive an error
  message as well as server logs.
- Focused evidence: configuration, capability advertisement, safe argv/stdin,
  unsaved-buffer, UTF-16 range, editor-application lifecycle, unchanged output,
  and failure tests pass.
- Milestone state: all Everyday LSP Completeness feature items are implemented.
  Production packaging blockers still prevent treating the 7.3 target as fully
  achieved.
- Next priority: align Cargo/npm/VSIX versions and resolve shipped npm audit
  findings before beginning broad Production Extension Platform expansion.
- Rating: current estimate is **7.2/10**. Everyday editor breadth has materially
  improved, but version-inconsistent artifacts and dependency vulnerabilities
  remain real production-readiness gaps.

### July 2026: Distribution consistency and installed-artifact verification

- Version consistency: Cargo, the VSIX manifest and lockfile, the npm CLI, all
  npm platform packages, and the CLI's optional dependency pins now agree on
  `0.2.6`.
- Regression prevention: `editors/check_package_versions.js` fails with every
  mismatched manifest and runs before VSIX packaging and npm publication. VSIX
  creation also requires the exact Cargo-versioned artifact name instead of
  moving an arbitrary `.vsix` file.
- Dependency safety: the vulnerable transitive `minimatch 5.1.6` and
  `brace-expansion 2.0.1` packages were upgraded to `5.1.9` and `2.1.2`;
  `npm audit --audit-level=low` reports zero vulnerabilities.
- npm evidence: the current platform binary and CLI are packed, installed into
  a clean temporary npm project, resolved through the public wrapper, and used
  to complete a real JSON-RPC/LSP initialize handshake.
- VSIX evidence: packaging now produces `ruby-fast-lsp-0.2.6.vsix`; inspection
  confirms manifest version `0.2.6`, the current binary, zipped stubs, bundled
  RSpec Wasm extension, and the remediated dependency versions.
- Milestone state: Everyday LSP Completeness and its production packaging gate
  are achieved for the declared scope.
- Next priority: audit the existing extension implementation against every
  Production Extension Platform criterion, then close the highest-impact
  contract, lifecycle, determinism, permission, SDK, and black-box gaps.
- Rating: current estimate is **7.3/10**. This reaches the Milestone 1 target
  based on tested daily editor breadth and installable artifacts; Rails,
  templates, extension-platform maturity, and measured production evidence
  remain necessary for higher ratings.

### July 2026: Deterministic extension identity precedence

- Correctness: only one valid extension package may execute for a manifest
  `id`; lower-priority duplicates are rejected before event dispatch or semantic
  patch application.
- Precedence: initialization-option sources override environment sources,
  explicit package paths override packages found through a directory within the
  same source, and filesystem path provides the deterministic final
  ordering.
- Failure behavior: an invalid higher-priority package does not reserve its ID;
  the next valid candidate may load. A valid winner cannot be displaced by a
  lower-priority duplicate.
- Focused evidence: a regression fixture loads identical RSpec Wasm bytes from
  reverse-lexicographic environment and initialization packages with different
  manifest versions, proving that exactly the configured package wins rather
  than both executing or path order deciding.
- Rating remains **7.3/10**. This closes one production extension-registry
  invariant, but Milestone 2 still lacks complete lifecycle events, project-
  local discovery, settings/watchers/process hosting, wall-clock isolation,
  broader patch families, and a third-party acceptance fixture.

### July 2026: Wasm wall-clock isolation and slow status

- Isolation: every Wasm guest boundary is now protected by both deterministic
  fuel and a 500 ms wall-clock deadline, including module instantiation,
  allocation, ABI queries, event/index calls, and deallocation.
- Runtime design: each loaded extension owns one cancellable Wasmtime epoch
  ticker instead of spawning a thread for every DSL call on the indexing path.
  Dropping or reconfiguring the registry stops and joins those tickers.
- Failure behavior: an epoch interruption returns a recoverable error, disables
  only the offending extension, and leaves unrelated extensions and core editor
  features available.
- Observability: deadline failures are classified as `slow` with their error in
  `ruby-fast-lsp/extensions/status`; other guest failures remain `failed`.
- Focused evidence: an infinite-loop guest with a deliberately huge fuel budget
  is interrupted by the wall deadline in well under one second, alongside the
  existing ABI, payload, fuel, memory, RSpec Wasm, and status-classification
  tests.
- Rating remains **7.3/10**. Execution isolation is materially stronger, but
  Milestone 2 still requires complete lifecycle/settings/watchers/process
  contracts, project-local discovery, broader patch families, SDK stability,
  and a minimal third-party acceptance fixture.

### July 2026: Extension activation, settings reload, and shutdown

- Lifecycle: every discovered Wasm guest receives bounded
  `lifecycle.activate` before it can handle indexing or request events. Guests
  that fail activation remain observable but cannot execute.
- Settings: activation receives the guest's value from `extensionSettings`;
  settings-only changes send `settings.changed` to the existing healthy guest
  without paying Wasm rebuild or ticker churn. A failed guest is recreated on
  the next change so corrected settings can recover it.
- Replacement and shutdown: discovery changes activate a replacement registry,
  swap it atomically, and deactivate the previous registry. LSP shutdown sends
  `lifecycle.deactivate` to every active guest.
- Safety: lifecycle events run under the existing payload, memory, fuel, and
  wall limits. Lifecycle callbacks may update private state but any returned
  semantic, response, or command patches disable that extension rather than
  silently mutating server state.
- SDK evidence: the mruby SDK now exposes `on_activate`,
  `on_settings_changed`, `on_deactivate`, and the current settings value.
  Focused host tests prove activation failure isolation, settings notification
  and recovery, and clean deactivation.
- Rating remains **7.3/10**. Milestone 2 still requires watcher routing,
  project-local discovery/trust, process hosting, broader patch families, SDK
  stability, and a minimal third-party acceptance fixture.

### July 2026: Trusted project-local extension discovery

- User-visible: trusted workspaces automatically discover manifest packages
  under `.ruby-fast-lsp/extensions/*` and recursively under `ruby_fast_lsp/**`.
  VS Code exposes `projectExtensionsEnabled`, forwards its workspace trust
  state, and reloads when trust is granted.
- Trust policy: project-local Wasm is fail-closed; an absent or false client
  trust signal loads nothing. Users may disable local discovery independently
  even in trusted workspaces. Explicit editor/bundled packages remain available
  in Restricted Mode.
- Determinism: configured/editor packages win over project-local packages,
  which win over environment/dev paths. Explicit packages beat directory
  discovery within a source, and filesystem path is the final multi-root
  duplicate-ID tie-break.
- Lifecycle: adding or removing workspace roots transactionally rebuilds the
  discovered set and deactivates replaced/removed guests through the existing
  bounded lifecycle path.
- Focused evidence: real LSP initialization discovers a trusted local RSpec
  package; tests cover untrusted and disabled rejection, both supported layouts,
  explicit-over-project precedence, root-order-independent duplicate identity,
  and dynamic root addition/removal.
- Rating remains **7.3/10**. Milestone 2 still requires watcher routing,
  process hosting, broader patch families/conflict policy, SDK stability, and a
  minimal third-party acceptance fixture.

### July 2026: Manifest-driven extension file watchers

- Registration: loaded manifests contribute validated workspace-relative globs;
  clients advertising dynamic watched-file registration receive their sorted,
  deduplicated union. Configuration and multi-root changes unregister stale
  patterns and register the replacement set.
- Routing: standard LSP file events are assigned to the deepest workspace root,
  normalized to relative paths, sorted/deduplicated, matched independently per
  extension, and delivered as a versioned `files.changed` domain event with
  created/changed/deleted kinds.
- Safety: absolute patterns, parent traversal, malformed glob syntax, and
  watcher declarations without the `watching` capability reject the package.
  Guest calls retain payload, memory, fuel, and wall limits; malformed output or
  traps disable only the receiving extension and remain visible in status.
- State boundary: watcher callbacks may refresh private route/schema/config
  caches but cannot return semantic, response, or command patches until a
  dedicated engine-owned ingestion contract exists.
- SDK/editor evidence: the mruby SDK exposes `on_watched_files_changed`; VS Code
  supports the server's dynamic registration alongside its Ruby source watcher.
  Focused tests prove handler-level matching/nonmatching behavior, failure
  isolation, nested-root selection, deduplication, glob validation, typed LSP
  registration, and SDK callback behavior.
- Rating remains **7.3/10**. Milestone 2 still requires process hosting,
  broader patch families/conflict policy, SDK/package-template stability, and a
  minimal third-party acceptance fixture.

### July 2026: Permission-enforced extension process host

- Public contract: the additive ABI v1 JSON contract now includes typed,
  bounded process requests and `process.completed` results; older guests remain
  compatible because the new output field defaults to an empty list.
- Trust and permissions: requests execute only in trusted workspaces, require
  the `process` capability and `process.exec` permission, must exactly match a
  manifest command allowlist, and may select only a workspace root related to
  the triggering file event.
- Isolation: the host launches commands directly without an implicit shell,
  limits request count, argv, stdin, timeout, and retained stdout/stderr, drains
  both streams to avoid pipe deadlock, and kills timed-out children. Policy
  violations disable only the requesting guest; process startup failures,
  nonzero exits, and timeouts return isolated results.
- SDK and evidence: the mruby SDK exposes `process_request` and
  `on_process_completed`. Focused Rust tests prove trust/permission/allowlist
  denial, successful bounded execution, and timeout termination; SDK tests
  prove request/result callback mapping.
- Rating remains **7.3/10**. This closes the external-process execution gap,
  while Milestone 2 still requires broader deterministic patch families and
  conflict policy, SDK/package-template stability, and a minimal third-party
  acceptance fixture.

### July 2026: Deterministic extension semantic-patch conflicts

- Provenance: every index patch is checked against the emitting guest's loaded
  manifest ID, so an extension cannot impersonate another extension in facts or
  status attribution.
- Semantic identity: method patches conflict by owner namespace/kind and method
  name; mixin patches conflict by source namespace/kind, target, and operation.
  The policy is independent of discovery order, traversal order, and timing.
- Merge policy: semantically equivalent patches are deduplicated with the
  lexicographically smallest extension ID retained for stable attribution.
  Incompatible patches are rejected before fact conversion and every
  conflicting guest is disabled with an observable status error, preventing
  ambiguous engine state.
- Architecture: guest provenance and conflict validation remain at the server
  extension boundary; `file_processor` converts only accepted domain patches
  through the normal per-file engine replacement path. No extension trust or
  ordering policy entered `ruby-analysis`.
- Focused evidence: red-first tests cover deterministic incompatible-method
  rejection, equivalent-mixin deduplication, and manifest provenance denial;
  the complete extension test group remains green.
- Rating remains **7.3/10**. The next Milestone 2 priority is expanding stable
  semantic patch families needed by a third-party acceptance extension, then
  shipping the SDK/package template and black-box authoring fixture.

### July 2026: Reusable extension SDK builder and third-party acceptance package

- Authoring surface: `extensions/mruby-sdk` now owns the package-agnostic mruby
  configuration, trap-only exception patch, Wasm C shim, direct builder, and
  reproducible Docker builder. The bundled RSpec extension delegates to this
  toolchain instead of owning a privileged or copied build path.
- Template: `extensions/example-dsl` is a copyable independent package using
  only the public Ruby SDK. Its `field :name` DSL emits an instance-method fact,
  document symbol, and code lens; source-level tests document expected payloads.
- Black-box evidence: the external LSP harness now supports definition lookup.
  A real initialize/open/query test loads the example manifest and proves the
  generated DSL method participates in engine-owned goto-definition alongside
  its document symbol and code lens.
- Reproducibility: the default acceptance test compiles a checked-in WAT/JSON
  representation of the public ABI, avoiding a redundant mruby dependency in
  the normal gate. The actual Ruby source was also built through the generic
  Docker toolchain and passed the same black-box test via
  `RUBY_FAST_LSP_TEST_BUILT_EXAMPLE=1`; this caught and removed an MRI-only
  regular-expression dependency from the template.
- Bundled compatibility: RSpec rebuilt successfully through the shared builder,
  its checksum was refreshed, both Wasm host acceptance tests passed, and its
  package validates through the public extension CLI.
- Rating increases to **7.4/10**. The reusable author path and independent
  semantic acceptance proof close meaningful Milestone 2 criteria. Reaching the
  7.7 milestone still requires broader stable semantic/response patch families,
  complete hook ingestion, and deterministic lifecycle replacement evidence.

### July 2026: Complete extension method metadata ingestion

- Correctness audit: `DefineMethodPatch` parameters were already retained, but
  declared visibility and return types were silently discarded during fact
  conversion. This made the public contract overstate what extensions could
  influence and weakened Rails-style generated API inference.
- Type ingestion: named extension return types now become engine-owned
  `TypeFact::MethodReturn` facts with `Extension` provenance and remain visible
  in signature metadata. Same-file collection mirrors the validated method and
  return type into local collector facts, so a later expression can infer the
  generated method without a second AST traversal.
- Visibility: public, protected, and private declarations now populate the
  engine `MethodFact` visibility used by centralized definition/reference/
  diagnostic policy. The black-box example proves a private generated method
  resolves as a bare call while an explicit receiver is rejected.
- Failure isolation: invalid method names, namespaces, parameter names, source
  ranges, mixin targets, and named return types are rejected at the untrusted
  guest boundary before fact conversion instead of reaching invariant panics.
- Lifecycle evidence: the independent external harness proves the generated
  method hovers as `String`, its private-call policy is enforced, and removing
  the DSL declaration through `didChange` removes both stale definition and
  return-type behavior through normal per-file replacement.
- Rating remains **7.4/10**. This makes an existing semantic patch family honest
  and production-usable; broader namespace, constant, attribute, type, and
  reference patch families are still required before Milestone 2 reaches 7.7.

### July 2026: Generated namespace and typed constant patches

- Public contract: additive `DefineNamespacePatch` and `DefineConstantPatch`
  variants let third-party guests declare generated classes/modules and typed
  value constants without engine or LSP access. The mruby SDK exposes matching
  `define_namespace` and `define_constant` helpers.
- Determinism and safety: namespace components, constant names, source ranges,
  named types, and manifest provenance are validated at the guest boundary.
  Declaration conflicts share FQN identity, so incompatible class, module, or
  value declarations are rejected deterministically before fact conversion.
- Engine ownership: accepted declarations become ordinary symbol, graph, and
  extension-provenance type facts through the normal per-file replacement path;
  the collector mirrors them during the active traversal for later references.
- Public acceptance evidence: the independent example package generates a
  class and typed constant; black-box LSP tests prove definition and `String`
  hover behavior and prove `didChange` removes stale namespace, constant, and
  type facts. The same test passes with both the deterministic WAT fixture and
  an actual Ruby-authored Wasm rebuilt through the public SDK.
- Rating remains **7.4/10**. The next Milestone 2 priorities are stable generated
  reference/relationship and richer type patch families, followed by complete
  lifecycle replacement and packaged-extension evidence.

### July 2026: Structured extension type contracts

- Public contract: extension types now represent named classes, arrays, hashes,
  unions, and unknown values as structured ABI data. The mruby SDK adds
  `named_type`, `array_type`, `hash_type`, `union_type`, and `nilable_type`,
  enabling Rails-style collection and optional generated APIs without parsing
  type-expression strings in the server.
- Safety and determinism: recursive conversion validates every Ruby name,
  rejects empty composite members, and bounds nesting depth and total nodes.
  Composite members are normalized and deduplicated before semantic conflict
  comparison, so equivalent guest types merge independently of member order.
- Engine evidence: the independent example extension declares a nilable
  `Array<String>` method return and `Hash<Symbol, String>` constant. Public
  black-box hover tests prove both reach existing engine type queries and stale
  composite facts still disappear through `didChange` replacement.
- Rating remains **7.4/10**. Rich generated signatures materially improve the
  extension foundation, but generated references/relationships and complete
  lifecycle/package evidence remain before the 7.7 milestone is justified.

### July 2026: Extension-generated declaration references

- Public contract: additive `AddReferencePatch` targets exact generated
  namespaces or value constants using domain names and source ranges. The
  mruby SDK provides reference and target helpers; guests never return LSP
  locations or access the reference store.
- Engine ownership: accepted patches create ordinary resolved
  `ReferenceCandidate` values during collection. Existing engine replacement,
  reference storage, and query policy handle them alongside parser candidates.
  A reusable exact-target query powers go-to-definition from the DSL token and
  refuses to guess when overlapping candidates disagree.
- Determinism and safety: targets and ranges are validated at the guest
  boundary. The same source token cannot resolve to incompatible extension
  targets based on package order; every conflicting guest is rejected through
  the established observable conflict path.
- Public acceptance evidence: find-references on the example's generated class
  includes the DSL argument, and go-to-definition from that argument reaches
  the generated class. Removing the declaration through `didChange` removes
  that generated reference while retaining ordinary source references.
  Boundary and engine tests cover invalid targets, deterministic incompatible-
  target rejection, exact lookup, and ambiguity refusal.
- Rating remains **7.4/10**. Declaration relationships now participate in core
  reference queries; complete lifecycle replacement/package evidence and the
  remaining Rails-oriented relationship vocabulary are still needed for 7.7.

### July 2026: Executable packaged-VSIX extension smoke

- Installed-artifact evidence: current-platform VSIX creation now extracts the
  produced archive, executes the binary from inside it, completes a real LSP
  initialize handshake, and queries extension status using the bundled RSpec
  package path from the same extracted artifact.
- Failure detection: packaging fails unless RSpec reports `loaded`, covering
  the packaged directory layout, manifest parsing, ABI compatibility, checksum,
  Wasm instantiation, activation, and status plumbing together.
- Isolation: the smoke child clears developer extension package/directory
  environment variables, preventing a local RSpec build from masking a broken
  or missing bundled package.
- Scope discipline: this is one release-artifact smoke, not a duplicate semantic
  suite; simulator and focused extension tests remain responsible for language
  semantics.
- Rating increases to **7.5/10**. The packaged VSIX completion criterion and
  several stable semantic patch families now have direct evidence. Milestone 2
  still needs explicit safe reload/replacement coverage and the remaining
  Rails-oriented relationship hooks before its 7.7 target is justified.

### July 2026: Content-aware transactional extension reload

- Correctness: extension discovery now fingerprints the ordered package source,
  precedence, path, parsed manifest, and Wasm bytes. A package replaced in
  place can no longer be mistaken for a settings-only update merely because its
  configured path is unchanged.
- Lifecycle: changed content is loaded and activated in a replacement registry
  before the atomic swap; the previous guest receives `lifecycle.deactivate`
  afterward. Unchanged content retains the cheaper `settings.changed` path and
  preserves guest state.
- Determinism: reload identity is content-based rather than filesystem
  timestamps, so identical package inputs behave consistently across machines
  and coarse timestamp filesystems.
- Focused evidence: a regression replaces the bundled RSpec fixture in place
  with a new manifest version at the same path, reuses the identical config,
  observes the new version as loaded, and observes the previous guest as
  deactivated.
- Rating remains **7.5/10**. Safe reload/replacement is now evidenced; remaining
  Milestone 2 work is the Rails-oriented relationship vocabulary and a final
  criterion-by-criterion extension-platform audit before considering 7.7.

### July 2026: Extension-generated superclass relationships

- Public contract: additive `SetSuperclassPatch` lets a guest connect a
  generated class to a relative or absolute superclass using domain names and
  a source range. The mruby SDK exposes `set_superclass`; no engine store or LSP
  type crosses the guest boundary.
- Ownership safety: the host accepts inheritance only when the same guest
  callback also emits a matching generated class declaration. Extensions cannot
  replace parser-owned superclass declarations. Namespace, target, range, and
  provenance are validated before fact conversion.
- Determinism: superclass identity is the generated class, so competing parents
  conflict regardless of extension or traversal order and all conflicting
  guests are rejected through the existing observable failure path.
- Engine integration: accepted relationships become ordinary resolved or
  unresolved `Superclass` graph facts, with singleton inheritance mirrored when
  immediately resolvable. Existing engine MRO, hierarchy, and method lookup
  remain the sole semantic policy.
- Public acceptance evidence: the independent example package generates
  `GeneratedRecord < BaseRecord`; black-box navigation resolves an inherited
  method through engine MRO, and `didChange` removal of the DSL declaration
  removes the stale superclass edge. The same test passes with the deterministic
  WAT fixture and a Ruby-authored Wasm rebuilt through the public SDK. SDK
  serialization, malformed target, ownership, deterministic conflict, RSpec
  compatibility, and the 55-test release simulator are green.
- Rating increases to **7.6/10**. The stable patch vocabulary now covers the
  explicitly planned namespace, constant, method/signature, structured type,
  reference, mixin, and inheritance relationships. A criterion-by-criterion
  Milestone 2 audit is still required before claiming its 7.7 target and
  beginning the Rails extension.

### July 2026: Milestone 2 completion audit and runtime reindex seam

- Audit result: all ten Production Extension Platform implementation items now
  have code and focused evidence: version/compatibility gates; complete manifest
  validation; domain-only contracts; deterministic discovery/merge/conflicts;
  activation/settings/reload/deactivation; failure isolation; Wasm/process
  limits and permissions; trusted multi-source discovery; the public SDK,
  template, documentation, and black-box harness; and declaration, response,
  watcher, and optional runtime hooks.
- Runtime gap closed: `ExtensionOutput` now has a version-compatible,
  default-empty `reindex_files` result. A `process.completed` callback may ask
  for at most 256 files under workspace roots related to its triggering event.
  Absolute paths, traversal, unknown roots, and unrelated roots are rejected;
  accepted URIs are sorted/deduplicated and enter ordinary watched-file
  processing.
- Architecture: runtime callbacks still cannot return semantic patches or touch
  engine state. They update private guest caches and request reindexing; normal
  call hooks then emit validated/conflict-resolved patches through per-file
  `replace_facts`. Static indexing remains useful without a runtime process.
- Completion-criterion evidence: RSpec and the independent example use only the
  public ABI; deterministic WAT and Ruby-authored Wasm acceptance cover semantic
  facts, symbols, lenses, navigation, types, and edit removal; incompatibility,
  checksum, permission, fuel, memory, wall deadline, malformed/oversized output,
  and traps are isolated; same-input ordering is deterministic; status is a
  separate command; the packaged VSIX loads bundled RSpec; and the copyable SDK
  documentation builds without server internals.
- Focused evidence: the runtime path has a red-first root/traversal validation
  test and an SDK callback serialization test. The preceding full gate proved
  973 root tests, all workspace tests, the 55-test release simulator, release
  build, packaged VSIX execution, and bundled-RSpec activation.
- Rating increases to **7.7/10**. Milestone 2 is complete for its declared
  scope. The next highest-priority work is Milestone 3: begin the Rails extension
  with static Active Record associations using only these public contracts,
  then validations/callbacks and route helpers. Optional runtime enrichment
  must use the bounded process/cache/reindex seam rather than privileged core
  access.

### July 2026: Static Active Record association foundation

- Rails package: `extensions/rails-ruby` is a bundled mruby Wasm guest using
  only the public SDK and `index.call` capability. No Rails macro names or
  framework resolution policy entered the core indexer or analysis engine.
- Association semantics: `belongs_to`, `has_one`, and `has_many` emit public
  reader/writer method facts, structured nilable or collection return types,
  and exact references from DSL arguments to conventionally inferred target
  classes. The engine remains the final owner of navigation and type queries.
- Lifecycle evidence: an external black-box LSP test proves association-target
  navigation, generated-reader definition lookup, structured hover output, and
  stale method/type removal after `didChange`. It passes both a deterministic
  WAT/JSON ABI fixture and the Ruby-authored Wasm built by the shared SDK.
- Distribution: VSIX packaging includes Rails beside RSpec, extension startup
  discovers both packages deterministically, and the archive smoke test now
  requires both guests to reach `loaded` status using their packaged Wasm.
- Deliberate limits: target inference is convention-only. `class_name`,
  `through`, `source`, polymorphic associations, namespaced inflection, and
  custom keys remain future Rails work rather than guessed semantics.
- Rating increases to **7.8/10**. This is the first user-facing Milestone 3
  slice. Reaching 8.1 still requires validations/callbacks, route helpers, and
  stronger Rails naming/options coverage; the next highest-value increment is
  association options followed by validations and callbacks.

### July 2026: Precise association options through the public ABI

- Generic contract: extension `Argument` values now carry optional keyword
  name/range metadata while retaining a distinct exact value range. Prism
  keyword hashes are flattened deterministically; positional and legacy ABI v1
  JSON remains compatible. The mruby SDK exposes `keyword_argument`, keyword
  accessors, boolean values, and an explicit unknown structured type.
- `class_name`: literal string or constant targets override convention-based
  inference, including namespaced models. The Rails guest emits its semantic
  reference at the option value, and generated reader/writer types use the
  exact fully qualified class.
- Polymorphism: `belongs_to ..., polymorphic: true` still generates reader and
  writer methods but emits no invented constant reference and uses `Unknown`
  return types. Unsupported explicit targets similarly avoid silently falling
  back to an unrelated conventional class.
- Architecture: keyword syntax extraction is framework-neutral at the server
  guest boundary. All Rails option interpretation remains in
  `extensions/rails-ruby`; engine method, reference, and type ownership is
  unchanged.
- Evidence: red-first Rails source tests cover both options; ABI tests cover
  exact metadata and legacy deserialization. Deterministic WAT and rebuilt
  Ruby-authored Wasm black-box tests prove namespaced navigation, typed hover,
  polymorphic non-navigation, generated methods, and edit lifecycle behavior.
- Rating increases to **7.9/10**. High-value association targeting is now
  credible without guessing. The next Milestone 3 priority is static
  validations and callbacks, followed by route/url helpers; `through`/`source`
  relationship enrichment remains an association follow-up.

### July 2026: Engine-owned callback and validation navigation

- Public method target: `AddReferencePatch` now supports an exact method owner,
  owner kind, and method name. The SDK exposes `method_reference_target`; guests
  still cannot inspect method facts or choose definitions.
- Single-sourced semantics: accepted targets become diagnostic-free ordinary
  method reference candidates. Engine method-reference resolution owns MRO,
  private visibility, ambiguity, reference storage, and later-definition
  behavior. Exact definition projection reads only these explicit candidates,
  so normal parser call visibility behavior is unchanged.
- Rails callbacks: the standard Active Record validation/save/create/update/
  destroy/commit/rollback/initialize/find/touch callback macros resolve literal
  symbol or string arguments to instance methods. `validate :method_name`
  receives the same behavior, including private custom validators. `validates`,
  `validates_associated`, and standard `validates_*_of` attribute helpers map
  positional names to reader method references without diagnosing missing
  schema-generated readers.
- Lifecycle and safety: callback symbols appear in ordinary find-references;
  removing declarations removes stale references through per-file replacement.
  Invalid method targets are rejected at the guest boundary, and overlapping
  incompatible targets retain deterministic conflict rejection.
- Evidence: red-first engine and Rails source tests cover the new contract.
  Deterministic WAT and rebuilt Ruby-authored Wasm black-box tests prove callback
  and custom-validation go-to-definition, private method resolution,
  find-references, and edit removal alongside existing association scenarios.
- Rating increases to **8.0/10**. Rails model navigation now covers its most
  common generated APIs and callback indirection. The next highest-value
  Milestone 3 work is route/URL helpers and route-to-controller navigation;
  richer validator classes and conditional validation semantics remain later
  refinements.

### July 2026: Static Rails routes and typed URL helpers

- Generic lexical frames: manifests may declare validated
  `[indexing].frame_call_names` independently from guest handlers. Frame calls
  retain literal and keyword arguments in `ResolvedCall`; activation state and
  deterministic manifest fingerprints still govern dispatch. This is a public
  DSL primitive, not a Rails-specific indexer hook.
- Route navigation: `resources`/`resource` declarations reference conventional
  or explicit controllers. Named HTTP routes and `root` parse literal
  `to: "controller#action"` targets into exact controller and engine-owned
  method references, with separate source subranges for each segment.
- Helpers: resource and named routes generate `_path` and `_url` methods on
  `ApplicationController`, with `String` returns and rest/keyword-rest
  signatures. Normal inheritance makes them available in controller subclasses;
  go-to-definition returns the route declaration and edits remove stale facts.
- Nested DSLs: `namespace` and `scope module:/as:` frame arguments produce
  deterministic controller/helper prefixes. A common explicit irregular
  singular set prevents wrong helpers such as `new_people_path` while avoiding
  a claim of complete Active Support inflection.
- Conservative limits: `only`/`except` routes emit controller navigation but no
  guessed helper subset. View/template helper projection, `member`/`collection`,
  shallow routes, mounted engines, and complete inflection remain incomplete.
- Evidence: red-first SDK and Rails source tests cover frame arguments, nested
  scopes, route targets, helper metadata, and irregular names. Deterministic WAT
  and rebuilt Ruby-authored Wasm black-box tests prove controller/action
  navigation, inherited typed helpers, namespace prefixes, and edit removal.
- Rating remains **8.0/10**. This is meaningful Milestone 3 breadth, but its 8.1
  target is not yet earned while controller-to-view navigation, Active Support
  concerns, Active Job entry points, and Minitest workflows remain incomplete.
  The next priority is Active Support concerns and Active Job, followed by
  Minitest discovery and test commands.

### July 2026: Active Job entry points and concern MRO evidence

- Active Job navigation: `perform_later` and `perform_now` on literal constant
  receivers ending in `Job` emit exact method-reference candidates targeting
  that job's instance `perform`. Engine method resolution remains responsible
  for identity, visibility, ambiguity, definitions, and find-references.
- Conservative scope: local/dynamic receivers and non-`Job` constants are
  ignored rather than guessed, preventing similarly named non-Rails APIs from
  acquiring synthetic navigation.
- Active Support concerns: model DSL facts declared inside a concern's
  `included` block stay on the concern module and reach including models through
  the existing parser-produced include edge and engine-owned MRO. No Rails guest
  duplicates mixin or ancestor lookup policy. Existing simulator coverage also
  proves `class_methods` block definitions become class methods on includers.
- Lifecycle evidence: deterministic and rebuilt Ruby-authored Wasm black-box
  tests prove enqueue-to-`perform` navigation, ordinary method references,
  concern-provided typed association lookup through an includer, and stale-fact
  removal after edits. Rails source tests prove both enqueue entry points and
  reject dynamic receivers.
- Deliberate concern gap: concern-specific dependency declarations remain
  incomplete; existing instance and `class_methods` composition covers the
  ordinary navigation workflow without a privileged Rails indexer hook.
- Rating remains **8.0/10**. Active Job closes another frequent Rails navigation
  path and common instance-side concern composition is now evidenced, but the
  8.1 milestone still lacks controller-to-view navigation and complete test
  workflows. The next highest-priority cohesive work is Minitest discovery plus
  run/debug commands, shared with the existing RSpec command surface.

### July 2026: Bundled Minitest discovery and real test debugging

- Minitest package: the independent bundled `minitest-ruby` Wasm guest uses only
  public document-symbol and code-lens contracts. It discovers conventional
  `*Test` classes, `def test_*` methods, and Rails `test "description"`
  declarations only in `test/` or `*_test.rb` files.
- Symbol ownership: ordinary class symbols remain core-owned; the guest adds
  only synthetic test declarations, avoiding duplicate document symbols.
- Execution: Rails targets use exact `bin/rails test file:line`; plain Minitest
  uses `bundle exec ruby -Itest file` with an exact method filter when one is
  available. RSpec continues to use `bundle exec rspec file:line`.
- Debugging: RSpec and Minitest Debug lenses now call VS Code's debugger with an
  `rdbg` launch configuration instead of displaying a placeholder notification.
  The documented prerequisites are the `debug` gem and a compatible Ruby
  debugger extension.
- Safety: terminal execution uses VS Code's structured `ProcessExecution`
  without an implicit shell; file URIs and line numbers are validated first.
- Evidence: red-first Ruby source tests cover discovery, exact lens arguments,
  and false-positive file filtering; a real-Wasm black-box LSP lifecycle test
  covers symbols, commands, and `didChange` removal; Node tests cover Rails/plain
  runner selection, class/method argv targets, and exact `rdbg`
  configurations. The packaged VSIX smoke requires Minitest, Rails, and RSpec to
  load from the extracted artifact.
- Verification: 975 root tests, all workspace tests including the new real-Wasm
  Minitest acceptance test, the 55-test release simulator, release build, Node
  command tests, and packaged VSIX initialization with all three bundled guests
  pass locally.
- Rating remains **8.0/10**. Milestone 3 still has one material gap:
  controller-to-view navigation, which should be designed with the ERB/source-
  mapping work rather than as a Ruby-only path guess. The next priority is that
  template/navigation seam, followed by Milestone 4 ERB support.

### July 2026: Offset-stable ERB analysis and controller-to-view navigation

- ERB projection: `.erb` files are indexed through an offset-preserving masked
  Ruby source. Host bytes are replaced one-for-one, newlines and Ruby tag bodies
  remain at their template offsets, executable tag boundaries become statement
  separators, and comments, escaped tags, and unclosed tags are not guessed as
  Ruby. UTF-16 conversion continues to use the original template document.
- Feature integration: definition, references, hover, completion, diagnostics,
  document symbols, semantic tokens, selection/folding ranges, inlay hints,
  rename analysis, and module code lenses parse the mapped source. Completion
  is suppressed at host positions, while edit/reindex removes stale ERB facts.
  RuboCop/Standard linting and formatting deliberately skip ERB until explicit
  host-language delegation exists.
- Editor surface: VS Code activates and synchronizes Ruby Fast LSP for the `erb`
  language. HTML request delegation is still incomplete and documented rather
  than being simulated by unsafe full-template Ruby edits.
- Rails views: the public Rails Wasm guest adds `Open View` lenses to public
  actions in conventional controller files. The guest emits controller/action
  domain arguments only; the VS Code adapter validates traversal-safe paths and
  opens the first existing `.html.erb`, `.turbo_stream.erb`, or `.json.jbuilder`
  candidate. Private actions and non-controller files are excluded.
- Architecture: no Rails filesystem lookup entered the indexer or engine, and
  no ERB coordinate policy entered the LSP adapter. The existing public
  extension response contract and `ruby-analysis::indexer` source ownership
  remain intact.
- Evidence: four mapper tests cover parsing, multibyte host text, trim markers,
  comments/escaped/unclosed tags; four root ERB lifecycle tests cover semantic
  navigation, completion isolation, local references, code lenses, formatting
  suppression, and reindex removal. Rails has 19 source-contract tests with 57
  assertions plus eight deterministic and rebuilt-Wasm black-box tests. Six
  Node tests cover process/debug commands and safe ordered view candidates.
  The full gate passes with 980 root tests, all workspace tests including 337
  `ruby-analysis` tests, the 55-test release simulator, release build, and a
  packaged VSIX initialize/status smoke with RSpec, Rails, and Minitest loaded.
- Rating increases to **8.2/10**. Milestone 3's implementation list is complete
  and Milestone 4's core ERB mapping/Ruby feature surface is usable. A manual
  representative Rails-app editor smoke remains missing evidence for the full
  Milestone 3 completion criterion. The next highest-priority Milestone 4 work
  is practical HTML delegation, followed by auditing `.rake`, `.gemspec`, Thor,
  generated/native declarations, and source-inclusion policy.

### July 2026: Range-safe HTML features inside ERB

- Host projection: the VS Code adapter now constructs a complementary HTML
  document that retains host markup and masks every complete or unclosed ERB
  region one UTF-16 code unit at a time while preserving CR/LF. It asserts that
  projected length is identical to the template, so HTML ranges require no
  translation and cannot drift around multibyte Ruby or host text.
- User-visible features: `vscode-html-languageservice` provides completion,
  hover, document symbols, folding ranges, selection ranges, and matching-tag
  highlights in ERB host regions. Ruby positions return no HTML completion,
  hover, or highlights; HTML selection at a Ruby cursor is deliberately reduced
  to an empty point so it cannot swallow embedded semantic ranges.
- Ownership: HTML UX remains entirely in the VS Code adapter. The Rust server
  continues to own Ruby semantics and its byte-stable Prism projection; no HTML
  types, policy, or dependency entered `ruby-analysis` or the engine.
- Conservative policy: whole-document HTML formatting and tag rename remain
  disabled until a range-safe merge policy proves they cannot overwrite Ruby.
  HTML diagnostics and links/colors remain future host-provider work with their
  own false-positive/lifecycle requirements. ERB is still never sent to RuboCop
  or Standard.
- Evidence: red-first Node tests cover missing delegation, UTF-16 projection,
  comments/escaped/unclosed tags, real HTML completion/hover/symbol/folding/
  selection/highlight results, Ruby isolation, and VS Code flat-symbol adapter
  conversion. The packaged smoke first rejected the previous VSIX for missing
  the service, then passed against the rebuilt artifact and exercised host
  completion plus Ruby suppression from the extracted package. The full gate
  passes with 980 root tests, all workspace tests including 337
  `ruby-analysis` tests, the 55-test release simulator, 10 Node tests, release
  build, zero npm audit findings, and packaged RSpec/Rails/Minitest status.
- Rating increases to **8.3/10**. Milestone 4's three template/editor items are
  complete for their declared range-safe scope. The next priority is auditing
  `.rake`, `.gemspec`, Thor, and other common Ruby entry points end to end,
  followed by generated/native declaration and source-inclusion policy.

### July 2026: Competitive common Ruby entry-point coverage

- Competitive audit: the current Shopify Ruby LSP VS Code manifest defines 24
  Ruby extensions, 22 conventional Ruby filenames, and three ERB extensions.
  Ruby Fast LSP previously discovered only a small server subset, advertised an
  even smaller editor subset, and watched only `.rb`/`.erb` changes.
- Canonical policy: `ruby_file_kinds.json` now records the shared set used by
  server discovery tests, VS Code language associations, filesystem watcher
  construction, Node contract tests, and packaged-artifact validation. This
  covers `.rake`, `.gemspec`, `.ru`, `.thor`, `.jbuilder`, `.rbi`, `.podspec`,
  related Ruby DSL extensions, and conventional names including `Thorfile`,
  `Fastfile`, `Dangerfile`, `Podfile`, and `.simplecov`.
- Lifecycle: cold workspace indexing now proves `Thorfile` and `config.ru` are
  registered and emit engine-owned symbol facts. VS Code watches both extension
  and conventional-filename groups, so unopened file changes reach the LSP
  instead of relying only on initial indexing or document-open events.
- Template aliases: `.rhtml` and `.rhtm` join `.erb` in server discovery, VS
  Code association, Ruby/HTML projections, completion isolation, diagnostics,
  and formatter/linter safeguards. Prism warnings are suppressed for embedded
  templates because output expressions are not void in the host; syntax errors
  and engine semantic diagnostics remain enabled.
- Evidence: red-first tests failed for `.ru`/Thor discovery, `.rhtml`
  definition, missing editor policy, and the stale packaged VSIX. Focused tests
  then proved policy parity, cold semantic indexing, legacy-template navigation,
  false-warning suppression, and formatting isolation. The full gate passes
  with 982 root tests, all workspace tests including 338 `ruby-analysis` tests,
  the 55-test release simulator, 12 Node tests, release build, zero npm audit
  findings, and an extracted VSIX whose manifest matches its packaged policy.
- Rating increases to **8.4/10**. Milestone 4 item 4 is complete at parity with
  the verified Shopify file-kind surface. The next priority is item 5: turn the
  existing generated, vendored, dependency, included/excluded, and trust rules
  into one explicit source-ownership policy with lifecycle and packaging
  evidence, then address native/generated declarations in item 6.

### July 2026: Explicit project source ownership and watched-file lifecycle

- Ownership contract: `ProjectFilePolicy` is the sole workspace path policy.
  Default Ruby sources are project-owned except beneath `vendor`, `.bundle`,
  `.ruby-lsp`, `.ruby-fast-lsp`, `node_modules`, `tmp`, `log`, and `coverage`;
  explicit includes can opt them in, explicit exclusions always win, and
  `.git` cannot participate.
- Semantic projection: `SourceKind` now exposes explicit workspace-owned,
  editable, diagnostic, reference, and dependency behavior. Workspace symbols,
  constant rename, the project-only namespace tree, and semantic diagnostic
  collection use those domain predicates instead of path guesses.
- Open-file safety: policy-excluded workspace files use
  `SourceKind::Excluded`. They receive interactive symbols and references while
  open, but do not become project-owned, editable, externally linted, or visible
  in workspace-symbol search; `didChange` preserves that ownership instead of
  silently promoting the file to `Project`, and `didClose` removes their
  interactive-only facts.
- Closed-file lifecycle: sorted create/change watcher events read and replace
  facts through the ordinary per-file engine write path, deletion or exclusion
  clears stale facts, and open documents remain owned by didOpen/didChange.
  VS Code watches configured include patterns as well as canonical Ruby file
  kinds and rebuilds those watchers when indexing configuration changes.
- Evidence: red-first tests proved that default discovery previously admitted
  vendored/cache/temp sources, workspace symbols exposed gem declarations,
  watched-file creation did not index facts, and opening or changing a vendored
  file promoted it to project ownership. The final gate passes with 984 root
  tests, all workspace tests including 339 `ruby-analysis` tests, the 55-test
  release simulator, 12 Node adapter tests, release build, zero npm audit
  findings, and a packaged VSIX initialize/status smoke with all three bundled
  extensions loaded.
- Rating increases to **8.5/10**. Milestone 4 item 5 is complete with explicit
  cold, watched, and interactive lifecycle evidence. The next highest-priority
  incomplete item is Milestone 4 item 6: define and prove the strategy for
  declarations supplied by native extensions or generated APIs.

### July 2026: Native and generated declaration strategy with project RBS overlays

- Static strategy: Ruby and RBI stubs remain ordinary parser-owned project
  declarations. Conventional `sig/**/*.rbs` files are now auto-discovered;
  additional RBS paths can use `includedPatterns`, exclusions still win, and
  `.git` remains impossible to opt in.
- Domain ingestion: `ruby-analysis::indexer::index_rbs` converts RBS classes,
  modules, methods, normalized `initialize` constructors, attributes,
  constants, visibility, mixins, inheritance, parameter metadata, composite
  return types, and source locations into ordinary analysis facts. The server
  registers `SourceKind::Signature` files and uses the single per-file engine
  replacement path; RBS never becomes an LSP-shaped or parallel semantic store.
- Overlay policy: native-only declarations participate in normal definition,
  completion, signature, type, MRO, and hover queries. When Ruby and RBS declare
  the same class or method, navigation prefers the executable Ruby definition,
  while signature help and otherwise-missing method return types use the RBS
  overlay. This avoids false ambiguity without discarding typed contracts.
- Generated lifecycle: VS Code watches `.rbs` without treating it as Ruby
  syntax. Closed signature create/change/delete events replace or remove facts;
  malformed regenerated RBS clears stale facts. DSL/runtime-generated APIs
  continue to use versioned public extension patches and bounded reindex
  requests, never direct engine writes.
- Evidence: red-first tests proved project RBS was previously undiscovered,
  watched RBS never entered the engine, and matching Ruby/RBS declarations
  produced ambiguous navigation with no type overlay. The completed tests prove
  cold discovery, include/exclude precedence, constructor normalization,
  definition and `String` hover from a real Ruby call, implementation-first
  navigation, RBS signature/type overlay, watcher replacement/deletion, and
  malformed-output cleanup.
- Verification: 987 root tests, all workspace tests including 341
  `ruby-analysis` tests, the 55-test release simulator, 12 Node adapter tests,
  release build, zero npm audit findings, and the packaged VSIX initialize and
  bundled-extension smoke pass locally.
- Rating increases to **8.6/10**. Milestone 4 is complete for its declared
  scope. The next milestone is measured production confidence: establish
  repeatable latency/memory budgets and semantic export fingerprints before
  making typing-path refresh changes.

### July 2026: Deterministic production latency and memory budgets

- Repeatable harness: the release `profiler` now has a production-benchmark
  mode over its deterministic generated corpus. It cold-indexes 172 analysis
  files / 2.4 MB of source, opens a representative controller through the real
  lifecycle handler, and measures full-buffer body-only edits plus completion,
  hover, definition, references, and semantic diagnostic projection.
- Evidence quality: query samples are accepted only when completion includes
  the expected method and hover/navigation return useful results. Percentiles
  use a tested nearest-rank calculation; `--check-budgets` fails when any cold,
  p95, or estimated-engine-heap budget is exceeded.
- Recorded budgets: 2 s cold indexing; 100 ms edit p95; 50 ms completion and
  references p95; 25 ms hover, definition, and diagnostics p95; 32 MiB
  estimated engine heap. `PERFORMANCE.md` records the command, corpus scope,
  measurement semantics, reference hardware, and change policy.
- Reference result: on an Apple M4 Pro with 24 GiB RAM and macOS 26.2, 100
  release iterations measured 695.8 ms cold indexing; 1.121 ms edit p95;
  0.074 ms completion; 0.081 ms hover; 0.062 ms definition; 7.606 ms
  references; 0.001 ms diagnostics; and 5.7 MiB estimated engine heap. Every
  budget passed.
- Rating remains **8.6/10**. This closes the repeatable budget and core p95
  measurement gap, but Milestone 5 still requires semantic export fingerprints,
  bounded visible/open-file diagnostic refresh, a reviewed diagnostic
  false-positive budget, simulator coverage audit, and release smoke evidence.
  The next priority is semantic export fingerprints so body-only edits can be
  proven distinct from API changes before refresh behavior changes.

### July 2026: Semantic export fingerprints and bounded typing refresh

- Engine contract: every per-file `replace_facts` computes a stable,
  order/range-independent fingerprint of exported classes/modules/constants,
  method signatures and visibility, exported constant/method/parameter types,
  and inheritance/mixin relationships. References, diagnostics, locals,
  expression types, method bodies, and declaration offsets cannot create a
  false API change.
- Lifecycle classification: the engine reports `InitialIndex`, `BodyOnly`, or
  `ExportsChanged`. `FileProcessor` compares the pre-pass and final fingerprints
  so its intermediate direct-fact seed cannot hide a real exported change.
- Typing path: body-only `didChange` work remains limited to the edited file.
  Exported API changes reprocess and publish diagnostics for at most eight
  deterministically sorted, project-owned open documents; closed project files
  never enter this path. This directly prevents the prior 2,186-file affected
  fanout failure mode.
- Correctness evidence: red-first engine and lifecycle tests prove declaration
  movement/body changes preserve the fingerprint, parameter changes alter it,
  removal of an exported method refreshes an open consumer's unresolved-method
  diagnostic, body-only edits do not touch another open file, and refresh
  selection is sorted and capped.
- Performance evidence: the post-change 100-iteration release benchmark passes
  every budget at 709.9 ms cold indexing, 1.511 ms body-only edit p95, 0.080 ms
  completion, 0.083 ms hover, 0.073 ms definition, 7.776 ms references,
  0.001 ms diagnostics, and 5.7 MiB estimated engine heap.
- Verification: 994 root tests, all workspace tests including 342
  `ruby-analysis` tests, release build, version/audit checks, and packaged VSIX
  initialization with bundled RSpec, Rails, Minitest, and ERB HTML features all
  pass locally.
- Rating remains **8.6/10**. Milestone 5 still needs the diagnostic
  false-positive budget, simulator-bucket audit, and release smoke-project
  evidence before the 9.0 rating can be considered.

### July 2026: Simulator diagnostic budget and real-project crash smoke

- Simulator completeness: the release simulator now runs 56 tests. Its explicit
  required-shape list has zero missing buckets, its navigation gap set is empty,
  and a new aggregate audit enforces zero engine semantic false positives over
  at least 50 oracle-owned valid call, constant, and macro sites. Code-less
  Prism syntax/style warnings remain a separate diagnostic class.
- Release smoke corpus: disposable local checkouts of Sinatra at
  `946812bdec8faf6598fed154a8d611ead612b6fd` and Discourse at
  `ca7f32c972e9f8b18c6ea47736e00787c3c5e0e2` exercise real dependency,
  stdlib, project discovery, fact collection, resolution, diagnostics, and
  memory paths without duplicating semantic assertions.
- Crash found and fixed: optimized indexing segfaulted on Discourse's Rakefile.
  A 146-byte regression proved `ruby-prism` 1.4.0's comment iterator crashes on
  raw leading `#!`. `mask_shebang` now changes only that `!` byte to `#` for
  every Prism analysis/comment parse, preserving all source offsets while the
  original document remains authoritative. Core and full file-processor tests
  reproduce the former crash and pass in release mode.
- Smoke results: Sinatra completes 618 analysis files / 5.0 MB in 1.34 s using
  15.2 MiB estimated engine heap. Discourse completes 11,159 files / 44.8 MB in
  9.47 s using 176.3 MiB. Neither has a known crash after the fix.
- Verification: 997 root tests, all workspace tests including 343
  `ruby-analysis` tests, the 56-test release simulator, release build, and the
  packaged VSIX initialize/extension/ERB smoke pass locally.
- Production risk discovered: Sinatra produces 1,891 engine diagnostics and
  Discourse 76,192. These raw counts are not a reviewed false-positive rate,
  but their scale contradicts any claim that real-project diagnostic precision
  is production-ready. The next priority is a conservative publication and
  precision policy measured against these projects; do not hide this evidence
  behind the simulator's narrower zero-false-positive budget.
- Rating is provisionally **8.7/10**: repeatable performance, bounded typing,
  simulator completeness, two real-project crash smokes, and a production crash
  fix justify a small confidence increase. The 9.0 milestone remains blocked by
  real-world diagnostic noise and the final clean/package release audit.

### July 2026: Open-document diagnostic publication policy

- User-visible policy: cold indexing no longer publishes semantic diagnostics
  for every closed project file. The reusable engine still retains every
  diagnostic fact for agent queries and future resolution, while `didOpen` and
  `didChange` publish current syntax/semantic diagnostics for active files.
- Scale impact: the 76,192 Discourse and 1,891 Sinatra engine facts no longer
  become an immediate Problems-panel flood. This is an LSP projection policy,
  not a semantic suppression heuristic, so it does not make false-positive
  evidence disappear or weaken engine correctness.
- Red/green lifecycle evidence: a focused coordinator test first proved cold
  indexing published a closed file's unresolved diagnostic. It now proves the
  fact remains in the engine, nothing is published while closed, and opening
  the file publishes the diagnostic normally.
- Rating remains **8.7/10**. Representative real-project open-file diagnostic
  precision is still unreviewed; the next step is to sample active Sinatra and
  Discourse files and harden only demonstrated false-positive classes.
