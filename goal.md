# Ruby Fast LSP: Production-Grade JRuby Support

## Reusable Goal Text

Elevate Ruby Fast LSP's JRuby support to an evidence-backed 9/10 production
level. The primary Java/JRuby navigation product is Go to Definition for
understanding how code is implemented, not merely proving that a symbol exists.
A JRuby project must receive the correct Ruby compatibility API, JRuby runtime
APIs, Java/JAR/JDK symbols, implementation navigation, hover, completion,
signatures, types, references, and diagnostics without mixing semantic state
between Ruby projects. Implement JRuby as a built-in runtime provider, not a
framework Wasm extension. Compose MRI compatibility stubs with versioned JRuby
additions, overrides, unavailable markers, and removals. Discover each owning
project's classpath deterministically, read bounded static classfile metadata
without executing project code, prefer exact Java sources for navigation, and
otherwise provide a deterministic read-only decompiled implementation view.
Generated signatures and stubs are semantic support and a final navigation
fallback only when neither exact source nor a verified decompiled member
location is available. Feed all accepted declarations and relationships into
the existing `ruby-analysis` fact and replacement lifecycle; do not create a
second Java semantic engine. Keep server configuration, internal safety policy,
framework extension configuration, and editor configuration transport
separate. Use strict TDD, simulator coverage for reusable Ruby semantics,
bounded JVM fixtures for classpath behavior, real `goshposh/admin` validation,
and packaged VSIX verification.

## Product Outcome

A production-ready result means a developer can open a mixed Ruby/JRuby
workspace and:

- Resolve ordinary Ruby core APIs according to the JRuby series' compatibility
  version.
- Resolve JRuby-only APIs such as `java_import`, `include_package`, Java proxy
  helpers, and runtime constants.
- Navigate imported Java classes and their constructors, methods, fields,
  superclasses, and interfaces.
- Receive overload-aware completion, hover, and signature help from JAR/JMOD
  metadata.
- Navigate to matching source from the project, `sources.jar`, or JDK `src.zip`.
  When exact source is unavailable, navigate to a deterministic read-only
  decompiled implementation view so Go to Definition remains useful for
  understanding behavior. Use a generated declaration-only signature only when
  decompilation cannot produce a verified member location.
- See precise diagnostics for absent APIs, unavailable runtime APIs, unresolved
  imports, ambiguous overloads, and incomplete classpaths.
- Use multiple isolated projects with different JRuby, JDK, gem, or JAR
  versions without cross-project symbol leakage.
- Select a runtime per discovered Ruby project through a clear editor workflow
  that distinguishes implementation version from Ruby compatibility version.

The target is **9/10 JRuby support**, not a complete Java IDE embedded in the
Ruby LSP. Java method-body semantic analysis, Java refactoring, and Java
compilation remain outside the required scope. Read-only decompilation for
implementation navigation is required, but decompiler output must never become
semantic truth.

### Primary Navigation Product Criterion

Go to Definition exists chiefly to answer “how is this implemented?” A stub or
generated signature can support completion, hover, type checking, overload
selection, and a last-resort declaration location, but it does not satisfy
implementation navigation when the selected member's exact source or bytecode
is available. Every JRuby/JVM navigation slice must therefore prove this order:
exact source first, verified read-only decompiled implementation second, and
declaration-only signature last. A release must not claim working Go to
Definition merely because a generated stub opens successfully.

This is a release-blocking product invariant:

- When exact matching Java source is available, navigation must open the exact
  selected class or member in that source.
- When source is absent but the selected bytecode member has an implementation,
  navigation must open that exact member in a checksum-verified, read-only
  decompiled document and expose its body.
- A generated signature/stub is acceptable only when the winning artifact has
  no safely recoverable implementation location: for example an abstract or
  native method, an interface declaration, a field/enum declaration, malformed
  or unsupported bytecode, or a bounded decompiler failure.
- Fallback must be decided per selected member, not once for an entire class or
  JAR. One unmappable overload must not demote other verifiable overloads to a
  stub.
- Opening an implementation document must retain the originating project,
  runtime, JDK, artifact, and classpath provenance so subsequent Go to
  Definition requests continue in the same semantic world.
- Stubs and generated signatures must never outrank an exact source or verified
  decompiled location for the same semantic identity.

The implementation-navigation acceptance test must assert meaningful body text
or an exact source member range. Merely asserting that an LSP `Location` was
returned, that a file opened, or that a declaration name is present is
insufficient.

## Current Baseline

Delivered foundations that must be preserved:

- Ruby implementation and compatibility-version detection, including
  `.ruby-version` identifiers such as `jruby-9.2.21.0`.
- Per-project `AnalysisEngine` isolation and longest-root request routing.
- Exact Bundler source identity and `java` platform gem selection for JRuby.
- Project-local `vendor/cache` fallback with exact lockfile and platform
  validation.
- Bundled MRI-compatible core stub selection.
- Ordinary external-source provenance, navigation routing, and signature source
  handling.
- Validated extension facts and deterministic per-file fact replacement.
- Packaged VSIX smoke testing and local real-workspace profiling.

Current implementation status:

- MRI compatibility stubs compose with explicit JRuby 9.0, 9.1, 9.2, 9.3,
  9.4, 10.0, and 10.1 overlays, including added, unavailable, and absent APIs.
- Default `BasicObject#method_missing` no longer suppresses valid unresolved
  method diagnostics.
- Isolated JAR/JMOD classpaths, Java proxy declarations, imports, aliases,
  descriptors, overloads, hover, completion, signatures, and lifecycle
  replacement are implemented.
- Go to Definition uses exact source per member, then verified bounded CFR
  output, then a generated signature whose declaration records why no
  implementation range was available.
- Selected JRuby runtime Ruby implementations such as `java_import` are
  checksum-verified and materialized from an allowlist inside `jruby.jar`;
  implementation and dependency sources outrank stubs and signatures.
- Packaged VSIX and npm install smokes, installed-artifact checksum
  verification, and the complete local gate passed on 2026-07-26. The goal
  slice is ready for its clean commit.

## Architectural Boundaries

### Repository ownership

```text
crates/
├── jvm-metadata/       # Pure bounded JVM class/JAR/JMOD metadata
└── jruby-support/      # JRuby names, imports, type projection, stub deltas

src/
├── runtime/jruby/      # Project discovery, classpaths, caches, lifecycle
├── semantic_patches/   # Shared validation/conversion/provenance
└── config/jruby.rs     # Canonical server-facing JRuby configuration

support/jruby/
├── stubs/              # Additive/override JRuby API declarations
└── compatibility/      # Absent and unavailable MRI API declarations

editors/vscode/vsix/
├── package.json        # Settings schema only
├── extension.js        # Configuration/status transport only
├── runtime_selector.js # Multi-step presentation over server runtime data
└── test/               # Editor transport and packaged-artifact tests only
```

Exact filenames may change during implementation, but ownership must not:

- `jvm-metadata` knows JVM structures but nothing about Ruby, LSP, editors, or
  workspace configuration.
- `jruby-support` knows JRuby import/proxy/type semantics but not VS Code or LSP
  protocol types.
- `src/runtime/jruby` owns per-project discovery, cache lifecycle, source
  materialization, bounded decompiler execution, logging, and configuration
  application.
- `ruby-analysis` remains framework/runtime neutral and owns final lookup,
  graph, reference, inference, and diagnostic truth.
- `extensions/` remains for framework and gem DSL integrations. JRuby support
  must not use `extensionSettings` or a privileged framework manifest.
- Editor adapters transport configuration and present status; they do not
  discover classpaths or interpret JRuby semantics.

### Single semantic write path

JRuby-generated declarations must become validated, file-owned ordinary facts
before entering:

```rust
engine.replace_facts(file_id, facts, resolve_mode);
```

Edits, deletes, classpath changes, project removal, and runtime changes must
remove stale JRuby facts through the same replacement lifecycle. A structural
classpath/catalog cache is allowed; a second symbol/reference/type engine is
not.

## Milestone 1: Diagnostic Correctness and Stub Composition

Implement a composed API model:

```text
MRI compatibility baseline
+ JRuby additions
+ JRuby signature/type overrides
- genuinely absent MRI declarations
+ present-but-unavailable API markers
```

Required work:

1. Fix default `BasicObject#method_missing` so it does not prove that arbitrary
   missing calls are dynamically supported. Genuine application/framework
   overrides must continue to suppress false diagnostics.
2. Move JRuby stub source-of-truth outside the VS Code adapter. Packaging may
   copy or archive assets, but editor directories must not own semantics.
3. Add one versioned overlay and acceptance lane for every modern JRuby
   compatibility series: 9.0, 9.1, 9.2, 9.3, 9.4, 10.0, and 10.1.
4. Cover `java_import`, `java_alias`, `include_package`, `java_package`,
   `java_send`, `java_method`, `to_java`, Java proxy entry points, and JRuby
   runtime constants.
5. Represent four outcomes explicitly: inherited baseline, added/overridden,
   absent, and present-but-unavailable.
6. Emit an actionable `unsupported-runtime-api` diagnostic for APIs such as
   `fork` that are known but unavailable, instead of pretending they are absent
   or fully supported.
7. Keep compatibility data deterministic and reviewable; do not copy the full
   MRI stub tree per JRuby release.

Required compatibility matrix:

| JRuby series | MRI compatibility baseline | Support policy |
| --- | --- | --- |
| 9.0.x | Ruby 2.2 | Full series overlay and acceptance lane |
| 9.1.x | Ruby 2.3 | Full series overlay and acceptance lane |
| 9.2.x | Ruby 2.5 | Full series overlay and acceptance lane |
| 9.3.x | Ruby 2.6 | Full series overlay and acceptance lane |
| 9.4.x | Ruby 3.1 | Full series overlay and acceptance lane |
| 10.0.x | Ruby 3.4 | Full series overlay and acceptance lane |
| 10.1.x | Ruby 4.0 | Full series overlay and acceptance lane |

Do not duplicate an overlay for every patch release. A series overlay is the
default, with bounded `introduced`, `removed`, or `changed` version intervals
for patch releases that actually alter the exposed API. Runtime detection must
select the exact interval deterministically. New JRuby compatibility series
must fail closed until their MRI baseline and JRuby delta are added and tested;
they must not silently reuse the nearest older overlay.

JRuby 1.7 and earlier are an explicit legacy boundary for the 9/10 goal. JRuby
1.7 can run different Ruby compatibility modes and follows a substantially
different runtime/JVM support model. It may receive a separately scoped legacy
package later, but must not complicate or weaken the modern provider.

Completion evidence:

- Focused stub composition and diagnostic tests.
- Black-box goto, hover, signature, completion, and diagnostic tests.
- No regression for universal APIs such as `Thread`.
- No `java_import` false diagnostic when the JRuby overlay is active.
- MRI and TruffleRuby projects do not receive JRuby-only declarations.

## Milestone 2: Bounded JVM Metadata

Add a reusable JVM metadata crate that reads declarations without decompiling
method bodies.

Required metadata:

- Class, interface, enum, annotation, record, and module identity where
  applicable.
- Superclass and implemented interfaces.
- Constructors, methods, overloads, fields, and enum constants.
- Visibility and static/final/abstract/native/synthetic flags.
- JVM descriptors, generic signatures, varargs, declared exceptions, and
  annotations.
- Parameter names when `MethodParameters` or suitable debug metadata exists;
  deterministic `arg0`, `arg1`, etc. otherwise.
- Inner/nested class identity and source filename/line metadata when present.
- Multi-release JAR selection for the owning project's active JDK.

Safety requirements:

- Never execute JAR contents, gemspecs, build files, or class initializers.
- Bound archive bytes, entry count, decompressed bytes, nesting, class count,
  constant-pool size, attribute size, and generated output.
- Reject malformed, duplicate, traversing, unsupported, or ambiguous entries
  with explicit errors.
- Fingerprint immutable inputs by content identity, not timestamps.
- Parse deterministically regardless of archive or filesystem ordering.

Completion evidence:

- Toolchain-independent checked fixtures for descriptors, overloads, generics,
  inner classes, interfaces, enums, corrupt archives, and multi-release JARs.
- Fuzz/property coverage for bounded parser entry points where practical.
- No Ruby, LSP, editor, or project configuration types in `jvm-metadata`.

## Milestone 3: Per-Project Classpath and Source Discovery

Classpath ownership follows the same isolated project root that owns the
Gemfile and `AnalysisEngine`.

Discovery order and inputs:

1. Active JRuby executable and its `JAVA_HOME`.
2. JDK runtime modules/JMODs and `lib/src.zip`.
3. Exact Java-platform gems selected from the owning lockfile.
4. `Jarfile`, `Jars.lock`, and `jar-dependencies` metadata.
5. Manifest `Class-Path` entries scoped to an already accepted artifact.
6. Trusted, explicit project-scoped additional classpath and source entries.

Requirements:

- Never merge classpaths across discovered Ruby projects.
- Preserve artifact precedence and report duplicate class identities.
- Support two projects using different versions of the same Java class.
- Watch accepted project classpath metadata and replace affected facts
  deterministically.
- Cache under a user cache directory keyed by canonical project identity,
  runtime/JDK identity, and artifact checksums. Never write generated state into
  the Ruby project.
- Do not run Maven, Gradle, Bundler, or arbitrary discovery commands implicitly.
  Any future process-backed discovery must be trusted, explicit, structured,
  bounded, and optional.

Configuration must remain small and project-scoped:

```json
{
  "jruby": {
    "mode": "auto",
    "projects": [
      {
        "root": "admin",
        "additionalClasspath": ["vendor/jars/*.jar"],
        "additionalSources": []
      }
    ]
  }
}
```

Internal parser/cache limits are not user settings. VS Code only mirrors the
canonical server configuration and restarts/reloads the appropriate projects.

## Milestone 4: Runtime Selection and Editor UX

Runtime identity has three independent dimensions:

```text
implementation → compatibility series → exact installed runtime
```

Examples:

```text
MRI → Ruby 3.3 → ruby-3.3.11
JRuby → 9.2 (Ruby 2.5) → jruby-9.2.21.0
JRuby → 9.4 (Ruby 3.1) → jruby-9.4.14.0
JRuby → 10.0 (Ruby 3.4) → jruby-10.0.6.0
JRuby → 10.1 (Ruby 4.0) → jruby-10.1.x
TruffleRuby → reported Ruby compatibility → exact installed runtime
```

The canonical server domain must expose a bounded runtime descriptor containing
the implementation, exact engine version, Ruby compatibility version,
discovery source, project applicability, support status, and executable
identity. Compatibility mapping is computed and validated once by the server;
editor adapters must not maintain a duplicate table.

VS Code must provide a `Ruby Fast LSP: Select Runtime` multi-step QuickPick:

1. Select the owning Ruby project when the workspace contains more than one.
2. Select Auto, MRI, JRuby, or TruffleRuby.
3. Select the implementation release family and its compatibility series.
   MRI labels use the Ruby series directly, such as `MRI 3.3`. JRuby labels
   show both identities, such as `JRuby 9.4 (Ruby 3.1)` and
   `JRuby 10.1 (Ruby 4.0)`. TruffleRuby labels show its release family and
   reported Ruby compatibility version. Any later supported implementation
   must follow the same rule instead of flattening engine and compatibility
   versions into one value.
4. Select an exact discovered installation, or choose a bounded explicit
   executable/version entry.
5. Show the effective project/runtime/JDK result before applying it.

The selector hierarchy must therefore render as:

```text
Project
└── Implementation
    └── Release family (Ruby compatibility)
        └── Exact discovered installation
```

This is a dynamic command-driven QuickPick backed by server discovery, not a
static `package.json` enum. Settings retain only the selected canonical runtime
descriptor or `auto`; they must not contain an editor-maintained catalog of
runtime versions.

Requirements:

- `auto` remains the default and respects the owning project's version-manager
  files, Gemfile requirements, and runtime environment.
- Explicit selection is stored per isolated project, not as one ambiguous
  workspace-wide Ruby version.
- Invalid implementation/version/compatibility combinations are rejected.
- Unsupported future series are shown as unsupported and fail closed instead
  of silently selecting a nearby stub overlay.
- Changing one project's runtime rebuilds only that project's runtime,
  classpath, stubs, and semantic state while preserving unrelated projects.
- A status command exposes the effective implementation, engine version, Ruby
  compatibility version, executable, JDK, stub overlay, and classpath
  fingerprint.
- Other editors can consume the same server runtime-discovery and
  configuration contract without reproducing VS Code-specific behavior.
- The old flat `rubyVersion` setting has a documented, deterministic migration
  path and remains backward-compatible for a bounded transition period.

Completion evidence:

- Unit tests for configuration parsing, migration, validation, and project
  routing.
- VS Code tests for every selector level, cancellation, no-installation,
  unsupported-version, multi-project, and restart/reload behavior.
- Packaged VSIX smoke proving the selector uses packaged code and the selected
  runtime reaches the server initialization state.
- Real selection of `goshposh/admin` as JRuby 9.2 with the effective Ruby 2.5
  compatibility overlay and owning JDK displayed correctly.

## Milestone 5: JRuby Import and Proxy Semantics

Support static, defensible forms of:

- `java_import java.util.concurrent.TimeUnit`
- `java_import "java.util.concurrent.TimeUnit"`
- import aliases supported by JRuby
- `include_package` and `java_package`
- canonical `Java::...` proxy namespaces
- Java interfaces included/implemented by Ruby classes
- statically named Java signatures and fields used by JRuby integration APIs

Project Java metadata into ordinary semantic identities:

- Imported constants in the correct lexical namespace.
- Java proxy classes/modules and namespace aliases.
- Constructors as `.new`.
- Static and instance methods with overload sets.
- Static fields and enum constants.
- Superclass/interface graph relationships.
- Primitive, boxed, array, generic, vararg, nullable/unknown, and proxy types.
- Exact Java method names plus JRuby aliases only when JRuby's mapping is
  deterministic.

Dynamic class names, runtime classloader mutation, reflection-generated
members, string evaluation, and ambiguous package imports must fail closed.
Never guess a class, overload, alias, or owner from indexing order.

Completion evidence:

- Definition, references, hover, completion, signature help, type inference,
  hierarchy, and diagnostics agree on one semantic identity.
- Removing or changing an import removes stale constants, methods, types, and
  references.
- Same-named imports in sibling projects remain isolated.
- Ruby declarations and Java imports follow tested JRuby collision semantics.

## Milestone 6: Navigation Documents

Navigation source precedence:

1. Exact matching project Java source.
2. Exact attached `sources.jar`.
3. JDK `src.zip`.
4. Deterministic read-only decompiled implementation source.
5. Generated read-only signature document for declarations that cannot be
   mapped safely into exact or decompiled source.
6. No location when artifact identity cannot be proven.

Go to Definition is primarily an implementation-understanding workflow.
Therefore, a generated signature is not considered successful source
navigation when bytecode for the selected class/member exists and can be
decompiled safely.

JRuby and Java stubs remain valuable semantic inputs for completion, hover,
signatures, types, and diagnostics, but they are not the primary navigation
product. A successful Go to Definition should show how the selected member is
implemented whenever exact source or safely decompiled bytecode is available.

Decompiler requirements:

- Use a pinned, license-compatible, checksum-verified decompiler artifact or an
  equivalently bounded in-process implementation. Package it with every
  distribution that advertises JRuby implementation navigation.
- Invoke it only through the selected project's exact JDK and exact winning
  classpath artifact. Never execute project classes, initializers, build tools,
  shell strings, or arbitrary user commands.
- Bound input size, process count, wall time, stdout/stderr, generated file
  count, output bytes, parser depth, and cache size. A timeout, crash, malformed
  output, checksum mismatch, or resource violation must fail closed for that
  document without corrupting the owning engine.
- Cache output under the isolated user cache using canonical project identity,
  runtime/JDK identity, winning artifact checksum, class identity, decompiler
  identity/version, and options. Never write generated output into the Ruby
  project.
- Parse the resulting Java source and map class, constructor, method, field,
  overload, nested-class, and line ranges back to exact classfile metadata
  identity. If a member cannot be matched unambiguously, use its generated
  signature location instead of guessing.
- Keep decompiled text presentation-only. Classfile/JMOD metadata remains the
  authority for names, descriptors, overloads, types, visibility, inheritance,
  diagnostics, completion, hover, and signature help. Decompiled bodies must
  never create or override semantic facts.
- Mark decompiled documents read-only, diagnostic-free, non-editable, excluded
  from project symbols/rename, and route them through the originating isolated
  project engine.
- Materialize only classes reached by navigation or bounded preflight; never
  eagerly decompile the full JDK or project classpath.
- Preserve external-document provenance so follow-up navigation from a
  decompiled document uses the same project, runtime, JDK, and classpath.

All navigation documents must:

- Preserve class/member identity and overloads.
- Use deterministic cache paths and content.
- Remain diagnostic-free, non-editable, excluded from project symbols/rename,
  and routed back through the originating project engine.
- Disappear when their owning classpath identity is no longer active.

Exact and decompiled Java implementation documents must use a read-only
external-source kind so they outrank declaration-only fallbacks. Generated
declaration documents alone use `SourceKind::Signature`.

Navigation completion evidence:

- Class, constructor, instance/static method, overloaded method, field, enum,
  nested-class, superclass, and interface navigation obey the precedence above.
- Every concrete fixture constructor and method whose bytecode has a body
  navigates to the exact source body or actual decompiled body, not merely a
  generated declaration such as `def ...; end`.
- Overload tests prove that the selected JVM descriptor reaches the matching
  source/decompiled member rather than the first same-named method.
- Abstract/native/interface members, fields, and enum constants prove their
  exact declaration location and explicitly record why no implementation body
  exists.
- A matching project source or `sources.jar` replaces the decompiled target
  deterministically.
- Removing or changing the winning artifact, source attachment, JDK, runtime,
  or project clears stale source/decompiled/signature files and locations.
- Packaged VSIX and npm distributions verify decompiler checksum/license assets
  and exercise implementation navigation without developer paths.

## Milestone 7: Production Evidence

Required test layers:

- Unit tests for descriptor, classfile, archive, compatibility, and name/type
  projection logic.
- Engine tests for any new framework-neutral alias, availability, overload, or
  provenance primitive.
- Black-box LSP tests for every supported JRuby workflow.
- Lifecycle tests for edits, deletes, watcher events, runtime changes, project
  add/remove, and external-document provenance.
- Multi-project tests with conflicting JRuby/JDK/JAR versions.
- Simulator coverage for reusable Ruby lookup and diagnostic semantics.
- Real profiling and manual acceptance on `goshposh/admin`.
- Packaged VSIX smoke proving JRuby assets and current native binary are present
  and usable without developer paths.

Real `goshposh/admin` acceptance includes:

- `java_import` itself resolves to the selected JRuby runtime implementation
  source when available, otherwise its exact runtime bytecode decompilation;
  an MRI-style declaration stub alone is not acceptance.
- `ServerMonitorListener`, `TimeUnit`, and `ConcurrentHashMap` resolve from the
  owning project classpath.
- Go to Definition on representative concrete Java constructors and methods
  opens exact source or a verified decompiled implementation body. Record the
  owning artifact checksum and selected JVM descriptor for each result.
- Imported constructors/methods provide useful hover, completion, and
  signatures.
- Ordinary Ruby core constants such as `Thread` remain correct.
- Cross-project `server` constants do not leak into `admin`.
- Cold indexing, didOpen, edit, query latency, and memory are measured and stay
  within recorded release budgets.

Recorded 2026-07-26 acceptance evidence:

- Project: `/Users/naveenraj/goshposh/admin`
- Runtime: JRuby 9.2.21.0, Ruby compatibility 2.5, JDK 17.
- Isolated catalog: 138 artifacts, 59,824 Java classes, 679 duplicate classes;
  classpath SHA-256
  `9b04dc48255c837b85513b13fcaea4e95379f796f435e821a424058aeb76f302`.
- Runtime/decompiler/source SHA-256 values:
  `04ea9921630ee03915fd7b50a0c6fd638301e7d9f72d13982a8711c3991e6660`
  (`jruby.jar`),
  `f686e8f3ded377d7bc87d216a90e9e9512df4156e75b06c655a16648ae8765b2`
  (CFR 0.152), and
  `6c41e630f42a41028c3affd51410136182e3944015f525a7b2af95ca906fc751`
  (OpenJDK `src.zip`).
- Cold indexing: 221.793 s for 16,581 files and 86,410,779 source bytes;
  estimated engine heap 242.9 MB.
- Unchanged-file reuse: 25.6-81.6 microseconds; complete didOpen waterfall:
  96.5-214.8 microseconds.
- `java_import` resolved uniquely to the selected runtime's materialized
  `jruby/java/core_ext/object.rb` implementation.
- `TimeUnit` and `ConcurrentHashMap` imports and aliases resolved uniquely to
  exact OpenJDK source. `TimeUnit.valueOf` used the generated signature only
  after the exact source and CFR output were both proven not to contain a
  mappable compiler-generated enum helper; the signature records this reason.
- `ServerMonitorListener` remained unresolved because `admin` has no owning
  MongoDB driver artifact. The sibling `server` project was not consulted.
- The current-platform VSIX and clean npm-install smokes both completed a real
  LSP handshake and JRuby implementation-navigation acceptance. The installed
  VSIX SHA-256 was
  `71f978dd8caf2d6df7178388ae6ececc667752c12d9f2ef50051916b47bc9184`;
  installed native binary, CFR, JRuby overlay, and runtime-selector hashes
  matched the packaged staging inputs.

## Definition of 9/10 JRuby Support

The rating may reach 9/10 only when:

- All seven milestones pass their acceptance criteria.
- JRuby 9.0, 9.1, 9.2, 9.3, 9.4, 10.0, and 10.1 have explicit tested
  compatibility policies, with patch-level exception intervals where needed.
- No known high-frequency JRuby interop workflow produces systematically false
  navigation or diagnostics.
- Classpath and generated facts are deterministic, bounded, isolated, and
  lifecycle-safe.
- Source and decompiled implementation navigation work from the installed
  artifact; concrete bytecode members navigate to meaningful implementation
  bodies, and generated signatures are observed only as a recorded, proven
  per-member last resort.
- The complete local pre-push gate passes.
- Representative real-workspace evidence is recorded.

The following may remain outside 9/10:

- Java method-body semantic analysis.
- Java code actions, formatting, compilation, or refactoring.
- Runtime reflection and dynamically mutated classloaders.
- Guaranteed recovery of parameter names stripped from bytecode.
- Perfect modeling of undocumented JRuby implementation quirks.

## Implementation Order

Do not begin with broad JAR indexing. Use this order:

1. Default `method_missing` correctness regression.
2. Stub delta data model and one JRuby 9.2 vertical slice.
3. Bounded classfile/descriptor parser.
4. Isolated classpath discovery for one fixture project.
5. Canonical runtime descriptor/configuration and the VS Code multi-step
   selector.
6. `java_import` end-to-end definition/hover/signature vertical slice.
7. Exact-source, decompiled-implementation, and generated-signature navigation.
8. Remaining modern JRuby series and import/proxy APIs.
9. Incremental lifecycle, performance, real-project, and packaged-artifact
   hardening.

Each slice follows red-green-refactor and must prove failure before the fix.
Avoid large mechanical coverage expansion until the first complete vertical
slice validates the architecture.

## Local Completion Gate

Hosted CI is not required. Before committing the completed milestone:

```bash
cargo fmt --all -- --check
cargo test
cargo test --workspace --exclude ruby-fast-lsp
cargo build --release
./editors/vscode/create_vsix.sh --current-platform-only
```

Also run the bounded JRuby real-workspace profiler and record the exact runtime,
JDK, project, artifact fingerprints, latency, memory, diagnostics, and query
results used for the release decision.
