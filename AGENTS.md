# Ruby Fast LSP - Codex Guide

This file provides context for AI assistants working on this project.

## Quick Reference

- **Project**: High-performance Ruby LSP written in Rust
- **Parser**: ruby-prism 1.4.0
- **Framework**: tower-lsp 0.20.0
- **Runtime**: tokio async

## Documentation

This file is the canonical AI guide. Keep it current when architecture,
testing, or agent workflow changes.

Supporting docs:

| File                                      | Purpose                                      |
| ----------------------------------------- | -------------------------------------------- |
| [src/ARCHITECTURE.md](src/ARCHITECTURE.md) | Current implementation architecture          |
| [src/query/README.md](src/query/README.md) | LSP query adapter boundaries                 |
| [src/test/README.md](src/test/README.md)  | Test harness and integration test structure  |

For Prism AST node names and byte offsets, prefer:

```bash
cargo run --bin ast -- '<ruby snippet>'
cargo run --bin ast -- --loc '<ruby snippet>'
```

## Common Commands

```bash
cargo test                    # Run all tests
cargo test -- --nocapture     # With output
cargo build --release         # Release build
./editors/vscode/create_vsix.sh --current-platform-only   # Build VS Code extension
./editors/scripts/smoke_npm_install.sh                    # Pack/install/initialize npm CLI
```

## Critical Reminders

1. **LSP positions are 0-indexed** - Line 1 in editor = line 0 in LSP
2. **Prism uses byte offsets** - Must convert to LSP positions
3. **FQN-based indexing** - All symbols use fully qualified names (e.g., `MyModule::MyClass`)
4. **AST Traversal** - Use recursive traversal (visitor pattern) over ad-hoc matching for type inference to handle nesting/chaining correctly

## TigerBeetle Principles (MANDATORY)

**CRITICAL**: This project follows TigerBeetle's philosophy of correctness over convenience:

1. **Fail Fast and Loudly** - Use `assert!` and `panic!`, NOT `debug_assert!`
   - ❌ **NEVER** use `debug_assert!` - bugs must be caught in production too
   - ❌ **NEVER** silently return wrong results or default values
   - ❌ **NEVER** use wildcard `_` in match arms for panics/unreachable - be explicit
   - ✅ **ALWAYS** panic with clear error messages explaining what went wrong
   - ✅ **ALWAYS** crash the program if an invariant is violated

2. **Make Invalid States Unrepresentable**
   - Use type system to enforce invariants at compile time
   - Use assertions to enforce invariants at runtime
   - Example: `assert!(matches!(fqn, Namespace(_, _)))` to validate enum variants

3. **No Assumptions or Guessing**
   - If data is missing or invalid, PANIC - don't guess what it should be
   - Better to crash and know there's a bug than silently produce incorrect results
   - Example: `.expect("INVARIANT VIOLATED: ...")` instead of `.unwrap_or_default()`

4. **Clear Error Messages**
   - Every panic/assert must explain:
     - What invariant was violated
     - Why this is a bug
     - How to fix it
   - Format: `"INVARIANT VIOLATED: <what> is broken. This is a bug because <why>. Fix: <how>"`

**Why**: Production correctness is more important than "graceful degradation" that hides bugs.

## Key Entry Points

- `src/main.rs` - Application entry
- `src/server.rs` - LSP server core
- `src/handlers/` - Request/notification routing
- `src/capabilities/` - Feature implementations
- `src/indexer/` - LSP/workspace indexing orchestration
- `crates/ruby-analysis/src/indexer/` - AST analysis and parser-to-facts code
- `crates/ruby-analysis/` - Shared facts, graph/query engine, inference, and parser-to-facts indexing primitives

## Agent-Focused Feature Tracking

Claude Code's agent-critical LSP requests are implemented: definition,
references, hover, document symbols, workspace symbols, implementation, call
hierarchy, and diagnostics after edits.

Verified gaps worth tracking:

| Gap | Current state | Why it matters |
| --- | --- | --- |
| Cross-file method rename | `rename` supports local variables, parameters, and project-owned classes/modules/constants. | Project-wide method rename still needs explicit ambiguity, alias, visibility, and dynamic-send policy. |

Do not treat old feature matrices as source of truth. Before adding a feature
gap here, verify it against `src/handlers`, advertised server capabilities, and
integration tests.

Signature help is implemented for user-defined methods through engine-owned
MRO and visibility resolution, with cross-file YARD metadata and RBS overload
fallback. Its LSP adapter supports nested calls, positional/rest arguments,
keywords/keyword-rest arguments, and edit/reindex lifecycle updates.

RuboCop and Standard diagnostics are available as opt-in external integrations.
They run on document open and save, consume the current buffer over stdin, merge
with semantic diagnostics, and stay out of the `didChange` typing path. The
server accepts a structured command argv and isolates startup failures, invalid
output, abnormal exits, and timeouts from analysis state.

Safe linter code actions are implemented as preferred `quickfix` actions for
correctable RuboCop/Standard diagnostics. They run against the current buffer,
use RuboCop `--autocorrect` or Standard `--fix`, and return a full-document
workspace edit only when corrected output differs. Unsafe RuboCop fixes and
failed/empty correction output never become edits.

Cross-file class, module, and value-constant rename is engine-backed and exposed
with prepare-rename support. Symbol facts retain exact declaration-name ranges;
the engine resolves constant identity, filters edits to project sources, rejects
invalid or colliding names, and returns deterministic definition/reference
ranges. The LSP adapter only maps those ranges to workspace edits. Rename range
conversion uses LSP UTF-16 positions, including non-BMP characters.

Document highlights are implemented as a same-document projection of the
existing semantic references query. They support constants, methods, and local
variables, refresh after edits, and intentionally do not introduce a separate
symbol-resolution policy.

Selection ranges are collected in `ruby-analysis::indexer` from Prism token and
AST containment ranges, then converted to nested LSP responses in `src/`. The
collector supports multiple positions, malformed and empty buffers, UTF-16
positions, and current unsaved document content.

Project indexing supports workspace-relative `includedPatterns` and
`excludedPatterns`, plus explicit `includedGems` and `excludedGems`. Standard
Ruby files remain the default; included patterns may add nonstandard source
names, exclusions always win, and `.git` is never traversed. Included gems
augment statically inferred roots, while excluded gems win over direct and
transitive requirements. The VS Code extension restarts the server after this
configuration changes so removed sources cannot leave stale engine facts.

Full-document formatting is available through opt-in RuboCop or Standard
integration. It consumes the current unsaved buffer over stdin, uses RuboCop's
safe `--autocorrect` or Standard's `--fix`, and returns one UTF-16-correct
full-document edit only when output changes. Formatter selection and command
argv are independent from lint diagnostics. Startup failures, abnormal exits,
timeouts, invalid output, and unsafe empty output produce no edit and do not
mutate analysis state.

Distribution versions are checked by `editors/check_package_versions.js` and
must match the root Cargo package across the VSIX, npm CLI, platform packages,
optional dependencies, and VSIX lockfile. VSIX creation and npm publishing fail
before packaging when versions drift. The npm install smoke test packs the local
CLI and current-platform package into a clean temporary project and proves the
installed wrapper can complete a real LSP initialize handshake.

Current-platform VSIX packaging must run `editors/scripts/smoke_vsix.js` on the
produced archive before moving it to `target/`. The smoke test extracts the
actual VSIX, executes its packaged platform binary, initializes it with the
bundled RSpec package path from that same extraction, and requires extension
status `loaded`. It clears developer extension-path environment variables so a
local package cannot mask a missing, invalid, or checksum-broken bundled copy.

Wasm extensions are bounded by payload, memory, fuel, and wall-clock limits.
Each loaded extension owns one cancellable Wasmtime epoch ticker; every guest
call boundary resets its fuel and 500 ms epoch deadline, including allocation
and deallocation exports. Deadline traps disable only that extension and appear
as `slow` through `ruby-fast-lsp/extensions/status`.

Extension guests activate before becoming eligible for indexing/request events.
Activation receives the extension's `extensionSettings`; settings-only changes
send `settings.changed` without recreating healthy guests. Discovery changes
transactionally replace the registry and deactivate the old guests, while LSP
shutdown deactivates the active registry. Lifecycle output may update private
guest state but must not contain semantic, response, or command patches.
Registry discovery stores a deterministic fingerprint of each discovered
package's parsed manifest, Wasm bytes, path, source, and explicit/discovered
precedence. Reconfiguration with unchanged paths uses the settings-only path
only when that fingerprint is also unchanged. In-place package updates activate
a replacement registry first, swap atomically, then deactivate the previous
guests; do not use timestamps as semantic reload identity.

Project-local extension discovery is fail-closed on workspace trust. Trusted
roots may contribute manifest packages from `.ruby-fast-lsp/extensions/*` and
`ruby_fast_lsp/**`; untrusted roots contribute none. Precedence is configured
or bundled packages, then project-local packages, then environment/dev paths,
with explicit packages before directory discovery and filesystem path as the
deterministic final tie-break across multi-root workspaces.

Manifest `[watching]` globs require the `watching` capability and must be valid
workspace-relative patterns without parent traversal. Supporting clients receive
a dynamically refreshed, sorted registration. Incoming file changes are scoped
to the deepest workspace root, normalized, sorted/deduplicated, matched per
extension, and delivered through bounded `files.changed` events. Watch callbacks
may update private guest state or request an external process, but cannot
directly return patches into engine or editor state. External process requests
require a trusted workspace, the `process` capability, `process.exec`
permission, and an exact manifest command allowlist. The server launches
commands directly without an implicit shell, scopes working directories to the
event's registered workspace roots, limits request/argument/stdin/output sizes,
caps timeouts at 10 seconds, and returns results through `process.completed`.
Policy violations disable only the requesting extension; spawn failures,
nonzero exits, and timeouts are isolated results and do not mutate analysis
state.

Extension index patches are validated and conflict-resolved in `src/extensions`
before `src/indexer/file_processor.rs` converts them into engine facts. Patch
`source.extension_id` must match the loaded manifest ID. Equivalent method or
mixin patches for the same semantic identity are deduplicated by extension ID;
incompatible patches are rejected and all conflicting guests are disabled.
Never let extension traversal, filesystem, or timing order decide semantic
truth, and never move this guest trust/provenance policy into `ruby-analysis`.

The reusable mruby authoring/build surface lives in `extensions/mruby-sdk`;
package-specific builders must delegate to it rather than copying the Wasm shim,
mruby configuration, or exception patch. `extensions/example-dsl` is the
independent public-contract template and acceptance package. Its black-box test
under `crates/lsp-test-harness` must prove manifest loading, a semantic DSL
method visible to engine-owned definition lookup, document symbols, and code
lenses without importing server internals. The deterministic WAT/JSON fixture
keeps the normal gate toolchain-independent; set
`RUBY_FAST_LSP_TEST_BUILT_EXAMPLE=1` after an SDK build to exercise the actual
Ruby-authored Wasm.

`DefineMethodPatch` metadata is semantic, not decorative. The extension boundary
validates method/namespace/type/range/parameter payloads before conversion.
`file_processor` must preserve declared visibility, signature labels, and an
extension-provenance `TypeFact` for declared returns. During the same file pass,
the extension host mirrors the method identity/visibility and return type into
the collector's local facts so later expressions can infer it without a second
AST traversal. Final facts still enter the engine only through per-file
`replace_facts`; edit/reindex removes stale extension methods and types.

`DefineNamespacePatch` and `DefineConstantPatch` are the public contracts for
generated class/module declarations and typed value constants. The extension
boundary validates Ruby names, types, ranges, provenance, and deterministic
declaration identity; a class/module/value declaration for the same FQN cannot
silently coexist with an incompatible patch. Accepted patches become ordinary
symbol, graph, and extension-provenance type facts through the per-file engine
write path. The collector mirrors them during the same traversal so later
references can resolve, and reindex removes all generated facts.

Extension `RubyType` values are structured domain data: `Named`, `Array`,
`Hash`, `Union`, or `Unknown`. Do not parse guest-provided type-expression
strings. The extension boundary recursively validates names, non-empty
collection/union members, maximum depth and node count, then converts to the
existing `ruby-analysis::core::RubyType`. Composite member order is normalized
before conflict comparison so equivalent unions, arrays, and hashes merge
deterministically. The mruby SDK exposes constructors including `nilable_type`.

`AddReferencePatch` lets an extension mark a source range as an exact semantic
reference to a namespace or value constant. The guest boundary validates the
target and range and conflicts patches by source range, rejecting incompatible
targets deterministically. Application creates a normal resolved
`ReferenceCandidate` in `FactCollector`; engine replacement resolves and stores
it with parser candidates, so references/highlights use existing query policy
and edits remove stale generated references. `AnalysisQuery` owns exact-target
definition lookup at a reference range and returns no result for ambiguity; the
LSP adapter only converts its definition ranges to locations. Do not return LSP
locations or write directly to the engine reference store.

## Architecture Direction: LSP Wrapper Over Engine + Inference

Long-term goal: `ruby-fast-lsp` should be a thin editor/LSP adapter over reusable
analysis crates. Editors are not the only consumers; agents and CLIs should be
able to ask graph/type questions without speaking LSP.

### Mandatory Layer Boundaries

When moving code, classify it by what it owns:

| Layer                      | Owns                                                                                                                                  | Must Not Own                                                            |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `ruby-analysis::core`      | Immutable contracts: FQNs, Ruby names, ranges, source IDs, facts, stores, Ruby types                                                  | AST traversal, query policy, LSP/editor protocol                        |
| `ruby-analysis::engine`    | Long-lived workspace semantic state, fact ingestion, cross-file graph/reference/diagnostic resolution, deterministic semantic queries | `tower_lsp` types, editor triggers, snippets, protocol response shaping |
| `ruby-analysis::indexer`   | Ruby source parsing and AST traversal that emits facts/candidates                                                                     | Global semantic truth, LSP protocol, workspace lifecycle                |
| `ruby-analysis::inference` | Type derivation rules, flow/local type tracking, RBS lookup/substitution                                                              | LSP protocol, editor UX, persistent workspace ownership                 |
| `ruby-fast-lsp src/*`      | Server lifecycle, document cache, LSP handlers/capabilities, editor-specific behavior, protocol conversion                            | Semantic truth, reusable type/graph algorithms                          |
| `extensions/*`             | External DSL/library knowledge that emits facts/patches                                                                               | Global source of truth; engine owns final graph/index state             |

Decision rule:

- If an API consumes/returns `tower_lsp::lsp_types::*`, `Url`, editor commands,
  snippets, trigger chars, or publish diagnostics, keep it in `ruby-fast-lsp`.
- If an API consumes/returns `TextRange`, FQNs, facts, graph relations, or
  `RubyType`, it belongs in `ruby-analysis`.
- Do not rename just because a term appears in LSP. Rename only when the name
  encodes an editor/client-specific projection. Examples:
  - Bad in engine: `CompletionCandidate` (completion is editor projection)
  - Good in engine: `MethodMatch`, `ConstantMatch`
  - Fine in engine: `CallHierarchy`, `TypeHierarchy` (domain/tooling concepts)

Current boundary status:

- `src/query/*` is a protocol adapter over `ruby-analysis::engine::AnalysisQuery`.
  Keep `TextRange -> Location`, cursor parsing, and LSP response shaping here.
- `ruby-analysis::engine` query code stays split by ownership (`lookup`,
  `hierarchy`, `namespace_tree`, `debug`, workspace symbol search).
- `src/capabilities/completion/mod.rs` owns trigger routing, snippets, and LSP
  completion flow. Reusable receiver/type probing and RBS candidate discovery
  live in `ruby-analysis::inference::completion`.

### Engine Store Direction

Keep the engine public API dead simple and composable. The engine is a fact DB:
files are registered, facts are replaced per file, deferred refs/diagnostics are
resolved in one batch, and all reads go through `AnalysisQuery`.

Public write API:

```rust
impl AnalysisEngine {
    pub fn new() -> Self;

    pub fn register_file(&mut self, file: SourceFileInput) -> SourceFileId;

    pub fn replace_facts(
        &mut self,
        file_id: SourceFileId,
        facts: FileFacts,
        mode: ResolveMode,
    );

    pub fn resolve(&mut self);

    pub fn query(&self) -> AnalysisQuery<'_>;

    pub fn stats(&self) -> AnalysisStats;
}

pub struct SourceFileInput {
    pub path: PathBuf,
    pub content: String,
    pub kind: SourceKind,
}

pub enum ResolveMode {
    Immediate,
    Deferred,
}
```

`register_file` owns only file registry/meta:

```text
path -> SourceFileId
SourceFileId -> SourceFile { path, content, kind }
```

`replace_facts` owns semantic reindex for one file: remove stale facts for that
file, insert new facts, clear caches, and optionally resolve immediately.

Initial workspace indexing should use:

```rust
for file in files {
    let file_id = engine.register_file(file.input);
    let facts = collect_facts(file_id, &file.content);
    engine.replace_facts(file_id, facts, ResolveMode::Deferred);
}
engine.resolve();
```

Single-file edit should use:

```rust
let file_id = engine.register_file(file.input);
let facts = collect_facts(file_id, &file.content);
engine.replace_facts(file_id, facts, ResolveMode::Immediate);
```

Do not merge `register_file` and `replace_facts` unless the engine also owns
parsing/fact collection. It should not. Fact collection needs `SourceFileId` for
`TextRange`, but parsing and AST traversal belong in `ruby-analysis::indexer`.

Private engine shape:

```rust
pub struct AnalysisEngine {
    files: FileStore,
    names: NameInterner,
    facts: FactArena,
    indexes: Indexes,
    caches: QueryCaches,
}

pub(crate) struct FileStore {
    by_path: HashMap<PathBuf, SourceFileId>,
    files: SlotMap<SourceFileId, SourceFile>,
}

pub(crate) struct NameInterner {
    fqns: Interner<FqnId, FullyQualifiedName>,
    methods: Interner<MethodId, RubyMethod>,
    constants: Interner<ConstantId, RubyConstant>,
    type_subjects: Interner<TypeSubjectId, TypeSubject>,
}

pub(crate) struct FactArena {
    symbols: SlotMap<SymbolId, SymbolFact>,
    methods: SlotMap<MethodFactId, MethodFact>,
    refs: SlotMap<ReferenceId, ReferenceFact>,
    types: SlotMap<TypeFactId, TypeFact>,
    diagnostics: SlotMap<DiagnosticId, DiagnosticFact>,
    graph_nodes: SlotMap<GraphNodeId, GraphNodeFact>,
    graph_edges: SlotMap<GraphEdgeId, GraphEdgeFact>,
}
```

Minimal v1 indexes. Add more only when profiler proves need:

```rust
pub(crate) struct Indexes {
    by_file: ByFileIndexes,

    symbols_by_fqn: HashMap<FqnId, Vec<SymbolId>>,

    methods_by_fqn: HashMap<FqnId, Vec<MethodFactId>>,
    methods_by_owner_name: HashMap<(FqnId, MethodId), Vec<MethodFactId>>,

    refs_by_target: HashMap<FqnId, Vec<ReferenceId>>,

    types_by_subject: HashMap<TypeSubjectId, Vec<TypeFactId>>,

    diagnostics_by_file: HashMap<SourceFileId, Vec<DiagnosticId>>,

    graph_nodes_by_fqn: HashMap<FqnId, Vec<GraphNodeId>>,
    graph_out: HashMap<FqnId, Vec<GraphEdgeId>>,
}

pub(crate) struct ByFileIndexes {
    symbols: HashMap<SourceFileId, Vec<SymbolId>>,
    methods: HashMap<SourceFileId, Vec<MethodFactId>>,
    refs: HashMap<SourceFileId, Vec<ReferenceId>>,
    types: HashMap<SourceFileId, Vec<TypeFactId>>,
    diagnostics: HashMap<SourceFileId, Vec<DiagnosticId>>,
    graph_nodes: HashMap<SourceFileId, Vec<GraphNodeId>>,
    graph_edges: HashMap<SourceFileId, Vec<GraphEdgeId>>,
}

pub(crate) struct QueryCaches {
    mro_by_namespace: HashMap<FqnId, Vec<FqnId>>,
    method_lookup: HashMap<(FqnId, MethodId), Option<MethodFactId>>,
    method_suggestion: HashMap<(FqnId, MethodId), Option<String>>,
}
```

Do not add an `IndexCore` wrapper; it adds ceremony without improving the mental
model. Keep the fields directly on `AnalysisEngine`.

Public query API should be small primitives that features compose:

```rust
impl AnalysisQuery<'_> {
    pub fn file(&self, file_id: SourceFileId) -> Option<&SourceFile>;
    pub fn file_id(&self, path: &Path) -> Option<SourceFileId>;

    pub fn symbols(&self, key: SymbolKey<'_>) -> impl Iterator<Item = SymbolView<'_>>;
    pub fn methods(&self, key: MethodKey<'_>) -> impl Iterator<Item = MethodView<'_>>;
    pub fn refs(&self, key: RefKey<'_>) -> impl Iterator<Item = ReferenceView<'_>>;
    pub fn types(&self, key: TypeKey<'_>) -> impl Iterator<Item = TypeView<'_>>;
    pub fn diagnostics(&self, key: DiagnosticKey) -> impl Iterator<Item = DiagnosticView<'_>>;

    pub fn graph(&self) -> GraphQuery<'_>;
}
```

Composable query keys:

```rust
pub enum SymbolKey<'a> {
    Fqn(&'a FullyQualifiedName),
    File(SourceFileId),
    All,
}

pub enum MethodKey<'a> {
    Fqn(&'a FullyQualifiedName),
    Owner(&'a FullyQualifiedName),
    OwnerName {
        owner: &'a FullyQualifiedName,
        name: &'a RubyMethod,
    },
    File(SourceFileId),
    All,
}

pub enum RefKey<'a> {
    Target(&'a FullyQualifiedName),
    Caller(&'a FullyQualifiedName),
    File(SourceFileId),
    AllProject,
}

pub enum TypeKey<'a> {
    Subject(&'a TypeSubject),
    File(SourceFileId),
    At {
        subject: &'a TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    },
}

pub enum DiagnosticKey {
    File(SourceFileId),
    AllProject,
    All,
}
```

Views should borrow facts or expose IDs; do not clone facts in hot read paths.
Public callers should not see arena/index IDs unless they explicitly need a
stable handle.

Rules:

- Public API sees domain facts/views/ranges, not store internals.
- Private core uses IDs, arenas, indexes, and caches.
- No public store getters such as `method_store()` or `graph_store()`.
- No direct `HashMap<FullyQualifiedName, Vec<FullFact>>` outside engine internals.
- One write path: `register_file -> replace_facts -> resolve`.
- Feature APIs compose query primitives, e.g. type hints from `types(File)` and
  project diagnostics from `diagnostics(AllProject)`.
- Method lookup semantics must stay single-sourced in the engine resolution
  module. Definitions, references, diagnostics, hover, call hierarchy, and type
  inference must not hand-roll ancestor/MRO lookup or duplicate "unique vs
  ambiguous vs missing" policy. Use
  `AnalysisQuery::resolve_method_callees*` for navigation-style answers and
  `AnalysisQuery::resolve_method_reference*` when the caller needs the
  diagnostic/reference policy:
  - `MethodLookupResult::Unique(MethodFact)` means a single callable method fact was found.
  - `MethodLookupResult::Ambiguous { owner, method }` means method exists but multiple defs match;
    references should resolve, unresolved-method diagnostics should stay silent,
    and signature/arity diagnostics should be skipped unless ambiguity is later
    disambiguated.
  - `MethodLookupResult::Missing` means the namespace exists but no matching method was found, so
    unresolved-method diagnostics may be emitted if the candidate asks for them.
- Migrate incrementally: hide stores first, then port methods, graph, refs,
  symbols, types, diagnostics. Start with method/graph because flamegraphs show
  method lookup, MRO, and unresolved-method suggestions as hot.

### Analysis Module Responsibilities

```text
ruby-analysis::core
  Shared data contracts only:
  FQN, RubyConstant, RubyMethod, RubyType, TextRange, SourceFileId,
  SymbolFact, MethodFact, GraphFact, ReferenceFact, DiagnosticFact, TypeFact.

ruby-analysis::engine
  Owns indexed facts and deterministic graph/fact queries:
  symbols, methods, refs, graph, diagnostics, workspace symbols,
  definitions/references, ancestors, implementors, namespace tree, debug views.
  It stores type facts already computed, but should not do heavy expression inference.

ruby-analysis::inference
  Owns type algorithms:
  literal/expression type inference, local flow/type tracking, narrowing,
  method return inference, RBS lookup/substitution.
  It depends on core/engine contracts, not on LSP.

ruby-analysis::indexer
  Owns parsing/fact collection from Ruby source. FactCollector should eventually
  live here, emitting facts/candidates into ruby-analysis::engine.

ruby-fast-lsp
  Thin wrapper:
  server lifecycle, document cache, LSP handlers/capabilities, VS Code/Zed
  adapter behavior, and mapping TextRange/domain results to LSP protocol types.
```

### Dependency Direction

Preferred:

```text
ruby-analysis::engine -> ruby-analysis::core
ruby-analysis::inference -> ruby-analysis::core
ruby-fast-lsp -> ruby-analysis::{engine, inference, indexer}
```

Avoid:

```text
ruby-analysis::engine -> ruby-analysis::inference
```

Engine should remain stable fact DB + graph/query layer. Inference should be
smart/pluggable and ask engine questions through a trait.

Sketch:

```rust
pub trait InferenceQuery {
    fn method_candidates(&self, receiver: &RubyType, method: RubyMethod) -> Vec<MethodFact>;
    fn type_fact(&self, subject: &TypeSubject, at: TextRange) -> TypeResolution;
    fn ancestors(&self, fqn: &FullyQualifiedName) -> Vec<FullyQualifiedName>;
}
```

`ruby_analysis::engine::AnalysisQuery` should implement this trait when the
inference/query seam is formalized.

### Migration Backlog

Moved non-LSP logic out of `src/`:

1. Done: `src/query/implementation.rs` delegates domain implementor lookup to
   engine and keeps only `TextRange -> Location`.
2. Done: `src/query/namespace_tree.rs` delegates snapshot/projection to engine
   and keeps command adapter behavior.
3. Done: `src/query/debug.rs` delegates debug/introspection queries to engine
   and keeps command response shaping.
4. Done: `src/query/references.rs` delegates target resolution/reference
   grouping to engine and keeps cursor identifier + `Location` mapping.
5. Done: `src/query/definition.rs` delegates symbol/method/global lookup to
   engine and keeps cursor identifier + protocol mapping.
6. Done: `src/query/completion.rs` maps engine/inference completion matches to
   LSP items; `src/capabilities/completion/*` keeps snippets, variables, and
   trigger plumbing.
7. Done: `src/query/hover/*` keeps protocol hover formatting while domain hover
   targets and semantic lookup live in `ruby-analysis`.
8. Done: analysis crates collapsed into one `crates/ruby-analysis` crate with
   internal `core`, `engine`, `inference`, and `indexer` modules.
9. Done: `src/inferrer/*` -> `crates/ruby-analysis/src/inference`.
10. Interim done: `FactCollector` moved under `crates/ruby-analysis/src/indexer/fact_collector`.
    Done seams: `ScopeTracker`, parser helper functions, and scope kind moved to
    `ruby-analysis::indexer`; collector validation emits `DiagnosticFact` instead
    of LSP diagnostics; `SourceDocument` owns source offsets/comments/TextRange
    conversion in `ruby-analysis::indexer`. Remaining: extract pure core after
    adding seams for `RubyDocument` variable scopes, extension hooks, and YARD
    parsing/type conversion.
11. Done: `src/analyzer_prism` compatibility facade removed. Analyzer,
    identifier lookup, document symbols, semantic tokens, rename, and analyzer
    tests now live under `crates/ruby-analysis/src/indexer`.
12. Done: `src/types` compatibility facade removed. Shared domain types are
    imported from `ruby_analysis::core` / `ruby_analysis::indexer`; Ruby version
    detection owns `RubyVersion` under `src/indexer/version`.

### Performance Backlog

- Indexing feels slow in real VS Code usage after extension packaging. Do not
  optimize mid-refactor; profile after architecture cleanup. Likely targets:
  duplicate parse/fact passes, full-file processing on every change, extension
  hook overhead, source offset conversions, and repeated engine graph resolution.
- Failed May 23 2026 experiment: incremental `ImmediateAffected` re-resolution
  during `didChange` regressed real editing. A helper edit fanned out to 2186
  affected files, splitting time between engine resolution and diagnostic
  publishing. Do not reintroduce broad inline affected-file fanout without a
  reproducible lifecycle benchmark and a design that distinguishes symbol export
  changes from body-only changes. Future direction: semantic export fingerprints
  plus bounded/visible-file diagnostic refresh, with project-wide refresh outside
  the typing critical path.

Rule of thumb: anything returning or consuming `tower_lsp::lsp_types::*`,
`Url`, editor commands, or publish diagnostics can stay in `ruby-fast-lsp`.
Anything returning `TextRange`, FQN, facts, graph entries, or `RubyType` belongs
in reusable crates.

## Testing

### Tag-Based Test Harness (`check()`)

Single-file tests use `check()` with inline tags. No fixtures needed:

```rust
use crate::test::harness::check;

#[tokio::test]
async fn my_test() {
    check(r#"
class User
  def name
    "hello"
  end
end

user = User.new
user.n$0
<complete items="name">
"#).await;
}
```

**Supported tags:**

| Tag                                    | Requires `$0` | Purpose                     |
| -------------------------------------- | ------------- | --------------------------- |
| `<complete items="a,b" excludes="c">`  | Yes           | Completion items at cursor  |
| `<hint label="...">`                   | No            | Inlay hint at position      |
| `<def>...</def>`                       | Yes           | Goto definition range       |
| `<ref>...</ref>`                       | Yes           | Reference range             |
| `<type>...</type>`                     | Yes           | Expected type at cursor     |
| `<err>...</err>`                       | No            | Expected error diagnostic   |
| `<err none>...</err>`                  | No            | Assert NO errors in range   |
| `<warn>...</warn>`                     | No            | Expected warning diagnostic |
| `<lens title="...">`                   | No            | Expected code lens          |
| `<th supertypes="A,B" subtypes="C,D">` | Yes           | Type hierarchy              |

Multi-file tests use `check_multi_file(&[("main.rb", "..."), ("other.rb", "...")])`.

### FakeEditor (Lifecycle/Re-indexing Tests)

FakeEditor routes all operations through the **real LSP handlers** (`handle_did_open`,
`handle_did_change`, etc.), ensuring tests exercise the exact same code paths as a real editor.

#### Tag-based assertions (simple cases)

```rust
use crate::test::harness::FakeEditor;

#[tokio::test]
async fn types_survive_reindex() {
    let mut editor = FakeEditor::new().await;
    let code = "a = [1, 2, 3].first";

    editor.open("test.rb", code).await;
    editor.check("test.rb", r#"a<hint label="Integer"> = [1, 2, 3].first"#).await;

    editor.set("test.rb", code).await;
    editor.check("test.rb", r#"a<hint label="Integer"> = [1, 2, 3].first"#).await;
}
```

#### Programmatic assertions (complex scenarios)

```rust
#[tokio::test]
async fn completion_filtering() {
    let mut editor = FakeEditor::new().await;
    editor.open("test.rb", "user = User.new\nuser.").await;

    // Type "na" after the dot
    editor.type_at("test.rb", 1, 5, "na").await;
    let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
    assert!(items.iter().any(|i| i.label == "name"));

    // Backspace and retype
    editor.backspace_at("test.rb", 1, 7, 2).await;
    editor.type_at("test.rb", 1, 5, "to").await;
    let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
    assert!(items.iter().any(|i| i.label == "to_s"));
}
```

**Lifecycle methods** (all async, route through real handlers):

- `editor.open("file.rb", content).await` — triggers `handle_did_open`
- `editor.set("file.rb", new_content).await` — triggers `handle_did_change`
- `editor.save("file.rb").await` — triggers `handle_did_save`
- `editor.close("file.rb").await` — triggers `handle_did_close`

**Editing methods** (simulate typing):

- `editor.type_at("file.rb", line, char, "text").await` — insert text at position
- `editor.backspace_at("file.rb", line, char, count).await` — delete before position

**Query methods** (return raw LSP results for programmatic assertions):

- `editor.complete_at(file, line, char)` — completion items (no trigger context)
- `editor.complete_with_trigger(file, line, char, ".")` — completion with trigger
- `editor.hover_at(file, line, char)` — hover information
- `editor.goto_def_at(file, line, char)` — definition locations
- `editor.references_at(file, line, char)` — reference locations
- `editor.inlay_hints(file)` — all inlay hints for file
- `editor.code_lens(file)` — all code lenses for file
- `editor.diagnostics(file)` — all diagnostics for file
- `editor.rename_at(file, line, char, "new_name")` — rename workspace edit

**Apply methods**:

- `editor.apply_edit(&workspace_edit).await` — apply rename/code action results
- `editor.content("file.rb")` — get current file content

**When to use FakeEditor vs check():**

- `check()` — single indexing pass, sufficient for most feature tests
- `FakeEditor` — lifecycle tests, completion filtering, multi-step scenarios, snippet testing

### FakeEditor vs External LSP Harness

There are currently two editor-test harnesses:

- `src/test/harness/fake_editor.rs` — internal full-featured `FakeEditor` for core
  tests. It supports tag checks, diagnostics, goto, refs, rename, workspaces,
  completion, editing, and direct access to core internals where needed.
- `crates/lsp-test-harness` — external black-box harness for package/extension
  tests that must exercise the public LSP initialization path.

Do not merge them casually: `crates/lsp-test-harness` depends on `ruby-fast-lsp`,
so root crate tests cannot depend back on it without creating a package cycle.
Future cleanup: rename the external one to `BlackBoxEditor` or `LspTestClient`
to avoid confusion, then keep the internal `FakeEditor` as the richer core test
harness until core tests move to external integration crates.

### Simulation Testing

Simulation tests are custom deterministic project simulations, not `proptest`.
They generate Ruby project graphs, drive lifecycle/edit operations through
`FakeEditor`, and compare LSP/index observations against a model oracle.

Useful commands:

```bash
cargo test test::simulation --release
SIM_SEED=123 cargo test generated_project_runs_seeded_edit_sequence -- --nocapture
SIM_RANDOM_SEEDS=10 cargo test generated_project_runs_seeded_edit_sequence -- --nocapture
SIM_LARGE_SCALE=1 cargo test generated_project_large_scale_smoke -- --nocapture
```

Seeded simulation uses fixed seeds plus `src/test/simulation/regression_seeds.txt`.
Failures write a replay artifact under the temp directory with the exact
`SIM_SEED=... cargo test generated_project_runs_seeded_edit_sequence -- --nocapture`
command. Add reduced regression seeds to `regression_seeds.txt`.

### Type Inference Architecture

**Two code paths for method return types:**

1. **Analysis engine** (`MethodResolver` path 1) — searches user-defined methods in ancestor chain
2. **RBS fallback** (`MethodResolver` path 2) — built-in Ruby types from RBS definitions

For generic types (`Array`, `Hash`), user-defined method lookup is **skipped** and RBS is used directly.
RBS handles generic substitution (e.g., `Array[Integer]#first` → `Elem` becomes `Integer`).

**Key files:**

- `crates/ruby-analysis/src/inference/type_tracker/mod.rs` — local flow/type tracking
- `crates/ruby-analysis/src/inference/rbs.rs` — RBS type lookup with generic substitution
- `crates/ruby-analysis/src/inference/completion.rs` — receiver type probing and RBS completion matches
- `src/query/completion.rs` — LSP completion item mapping for analysis matches

## Subagent Delegation

**Use Sonnet background subagents for mechanical work.** Reserve Opus for tasks that need critical thinking (design decisions, novel architecture, ambiguous tradeoffs).

**Mechanical = good fit for Sonnet:**

- TDD wiring of a new diagnostic that mirrors an already-shipped one (enum variant + emit + visitor branch + tests)
- Repetitive refactors across many files (renaming, splitting an enum variant, propagating a new field)
- Following a fully-specified plan where the design is decided

**Critical thinking = stay on Opus:**

- Choosing between competing architectures
- Designing a new abstraction or data model
- Diagnosing root cause of an unfamiliar bug
- Anything where the user's intent is ambiguous

**When dispatching to Sonnet, the prompt MUST include:**

1. Project root + reminder to read `AGENTS.md`
2. Recent commit SHA so it knows the baseline
3. Exact data shapes (enum variants, struct fields)
4. Skeleton implementations of helpers when shape is non-obvious
5. Reference to a similar shipped pattern (`mirrors raise-non-exception V2 — see commit X`)
6. All test cases written verbatim
7. Wire location (which file, where in the function)
8. Style reminders (TigerBeetle: assert!/panic!, no debug_assert!)
9. Required test count target after the change
10. Commit message
11. Don'ts list (no push, no unrelated changes)
12. **Tip: AST verification** — if Sonnet needs to verify Prism node names/accessors, point it at `cargo run --bin ast -- '<ruby snippet>'` (with optional `--loc` for byte offsets). Saves a roundtrip vs grepping the prism crate source.

**Parallelism:** When dispatching multiple Sonnet agents in parallel on overlapping files, use `isolation: "worktree"` so each gets an isolated git worktree. Single-task dispatches don't need worktree.

**Mid-flight diagnostics:** When Sonnet is wiring a new enum variant, expect transient non-exhaustive-match errors as it incrementally edits. These are normal and resolve when the agent finishes — don't treat them as the agent struggling.

## TDD Workflow

When the user reports that behavior is broken or "not working", follow this
strict TDD process. This applies even if the report is informal or diagnostic
driven rather than a polished code example:

1. **Red**: Create an integration test that captures the expected behavior
   - Write the test first based on the reported behavior
   - If the report lacks a complete snippet, reduce it to the smallest
     representative fixture that exercises the same LSP/indexer/engine path
   - Run the test to confirm it fails
   - Show the failing test output

2. **Green**: Implement the minimum code to make the test pass
   - If the change is substantial (architectural changes, new modules, cross-cutting concerns):
     - Use `EnterPlanMode` to design the feature
     - Ask clarifying questions about design decisions
   - Make targeted changes to fix the failing test
   - Run the test to confirm it passes

3. **Refactor** (if needed): Clean up while keeping tests green

**Important**: Always verify the test fails before implementing the fix. This validates the test actually tests the new behavior.
