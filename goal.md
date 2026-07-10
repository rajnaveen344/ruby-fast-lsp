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
7. Index include/exclude and dependency configuration.
8. Full-document formatting through external formatter integration.

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

## Milestone 3: Credible Rails Development

Target rating: **8.1/10**.

Build a first-class Rails extension supporting the highest-value workflows:

1. Active Record associations.
2. Validations and callbacks.
3. Route and URL helpers.
4. Controller-to-view and route-to-controller navigation.
5. Active Support concerns.
6. Active Job entry points.
7. Minitest and RSpec discovery.
8. Test code lenses and run/debug commands.

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

1. ERB parsing and stable source-range mapping.
2. Ruby LSP features inside ERB regions.
3. HTML request delegation in the VS Code extension where practical.
4. `.rake`, `.gemspec`, Thor, and common Ruby extension handling.
5. Clear generated, vendored, dependency, and excluded-source policy.
6. A strategy for declarations from native extensions or generated APIs.

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

- Keep Cargo, npm package, npm platform packages, and VSIX versions consistent.
- Ensure packaging output names match the version actually embedded in the
  artifact.
- Resolve shipped npm dependency vulnerabilities.
- Verify the server starts through the installed npm wrapper over stdio.
- Verify the packaged VSIX discovers its binary, stubs, and bundled extensions.
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
