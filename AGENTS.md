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

No agent-critical LSP request is currently known to be absent. Add a gap here
only after checking advertised capabilities, handlers, and integration tests.

Project-wide method rename is engine-owned and fail-closed. It uses exact
declaration-name ranges and namespace-kind identity, updates editable resolved
calls, static `send`/`__send__` targets, alias source operands, visibility
modifiers, inherited calls, and reopened definitions. It rejects external or
generated/macro declarations, unresolved lookup chains, ambiguous targets,
operator syntax, writer-shape changes, `super`-coupled override families, and
destination collisions across ancestor and descendant lookup chains. The LSP
adapter only validates the new Ruby name and converts engine ranges into edits.

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

Cold workspace indexing retains semantic diagnostic facts for all project files
but publishes only documents currently open in the client. Opening/changing a
file publishes its syntax and semantic diagnostics normally. Do not flood the
LSP client with closed-file diagnostics or weaken the engine's reusable
diagnostic store to implement this projection policy.

Missing-method diagnostics require a complete engine-owned lookup chain. If a
superclass, include, prepend, or extend edge is unresolved anywhere in the
candidate chain, navigation may retain its diagnostic-free reference candidate
but the engine must not claim the method is missing. The implicit Ruby
`Object` superclass is a known language root, not an explicitly unresolved
dependency edge. Ruby `def name(...)`
forwarding is a semantic parameter kind accepting positional and keyword
shapes. Attribute writer facts (`attr_writer`, `attr_accessor`, and
`class_attribute`) have one required value parameter. Calls using keyword
syntax satisfy a positional options-hash parameter when the method declares no
keyword parameters; keep these Ruby call-shape rules in engine diagnostics.

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
transitive requirements. Clients that expose these engine-level overrides must
restart the server after changing them so removed sources cannot leave stale
engine facts.

The VS Code Settings page exposes only `rubyFastLsp.logLevel`. Runtime
selection, linter selection, formatter selection, and Ruby Index external-type
visibility are editor commands backed by private workspace state; they must not
be serialized into user `settings.json`. The adapter sends deterministic server
defaults for indexing policy, JRuby attachments, extension settings, custom
tool argv, and project-local extension enablement. Keep internal initialization
transport fields out of the contributed configuration schema.
The bottom-right runtime status follows the active document's deepest owning
project and opens the project runtime workflow. An explicit runtime selection
persists privately for the VS Code workspace. Saving it to `.ruby-version` is a
separate confirmed project write that switches the project to Auto; the server,
not the editor, resolves that exact marker against one bounded discovered
runtime catalog shared by the isolated project coordinators. Never let the
editor reconstruct implementation/compatibility mappings or silently choose a
nearby runtime.
The indexing detail picker joins the exact server-reported runtime, JDK, and
classpath identity for each isolated project with the authoritative indexing
snapshot. Persistent gem/Java/compiled-Wasm counters plus gem, classpath-file,
and parsed Java-artifact single-flight reuse are process-lifetime evidence
because identical immutable work can be shared across isolated engines;
display them as process-wide totals and never attribute global counter deltas
to one project.
The VS Code indexing-status session is transport-scoped. Suspend acceptance
before restarting the language client, coalesce concurrent restart callers,
reset the sequence only after the old transport has stopped, and then refresh
one authoritative snapshot. Disposal permanently rejects delayed
notifications. Never carry a previous server process's sequence into a new
server, whose sequence begins again at one.

An editor workspace folder is a project container, not necessarily one Ruby
project. A root `Gemfile` owns the folder. Without one, discover the nearest
nested Gemfiles deterministically, prune `.git` and default-external trees, and
stop below each discovered root. `indexing.projectRoots` explicitly overrides
automatic discovery for umbrella repositories; entries must be relative,
workspace-contained, existing, and non-overlapping. Git topology never defines
Ruby semantic topology. Every discovered project owns an isolated
`AnalysisEngine`, Bundler/stdlib inputs, diagnostics, and extension facts.
Extension manifest semantic targets must therefore be seeded once per isolated
engine, not once per shared extension registry. Opening an already cold-indexed
project file with byte-identical content reuses its existing facts and runs only
the open-document syntax/diagnostic projection; changed content still takes the
ordinary per-file replacement path. This reuse path must not refresh other open
documents; newly indexed files may still refresh open consumers until engine
dependency tracking can replace that correctness-preserving fallback.
Document queries and watchers route by longest project-root prefix;
workspace-symbol search aggregates isolated engines deterministically. Dynamic
folder add/remove must rehome open documents between project and orphan engines
and clear their stale facts from the previous owner.
While active-project navigation is pending, its source pass owns all but one
bounded indexing CPU lane. Exact immutable gem discovery for that same project
may use the remaining lane, second task admission, transient-memory partition,
and I/O slot; sibling project-parallel passes remain blocked by the active
navigation reservation. Discovery returns resolver state to the owning
coordinator, and semantic product binding still occurs only through the
project's isolated engine.
Release that five-lane reservation at the accepted project-navigation frontier.
Keeping it through the exhaustive active-project tail was measured and rejected:
it improved the early frontier but regressed active dependency/semantic readiness,
terminal wall time, and peak memory, including one fixed-ceiling breach. Do not
repeat that scheduling shape without a materially different profile-backed design.

Bundler discovery must use the owning project's exact `Gemfile`. If a locked
Git dependency has no normal Bundler checkout but has an extracted
`vendor/cache/<repository>-<revision>` source, parse its `GIT` lockfile section
without executing the cached gemspec and index only its `lib` tree as
`SourceKind::Gem` in that project's engine. Never promote a vendor cache into a
project root or editable project truth. A locked registry dependency may use
the exact platform-qualified `.gem` archive in that same project's
`vendor/cache` only when no exact installed name/version/platform source is
available. Gem selection is per lockfile identity and explicit source kind:
Bundler-installed exact source first; registry locks may then use an exact
active-Ruby installation and finally an exact project archive; Git locks may
use only an exact Bundler checkout or matching extracted Git cache; path locks
require Bundler's exact path source. Never choose the highest unrelated global
version or substitute registry, Git, and path sources for one another.
Bundler lockfiles may contain both `ruby` and `java` variants for the same
registry gem. These are valid platform alternatives, not conflicting source
identities: select the `java` identity for JRuby and the non-Java identity for
other engines, while retaining exact version/source matching and rejecting
genuinely ambiguous variants.
Automatic installed-gem discovery performs Bundler resolution and its global
RubyGems fallback inside one invocation of the exact selected runtime. The
child result carries an explicit source marker so Bundler-installed and global
sources retain their distinct selection precedence; do not restore a failed
Bundler process followed by a second runtime startup.
Standalone Ruby project roots without a `Gemfile` must skip automatic installed
gem discovery entirely and index only project, core-stub, and available stdlib
inputs. A plain `require` in such a project does not authorize searching the
active runtime's global gems. Explicit `includedGems` remains the sole unlocked
exception and may trigger one governed global discovery after the project scan
has established the explicit selection set.
Explicit `includedGems` is the sole unlocked exception and may select the
active Ruby's highest installed version by deliberate user request. Validate a
project archive's package metadata against the lockfile, extract only declared
require paths without executing package code, and index them as
`SourceKind::Gem`. Extraction belongs in the user cache under a
canonical-project identity and archive checksum; never write generated
extraction state into the Ruby project or share semantic ownership across
isolated projects.

Bundled core Ruby stubs are language semantics and must be indexed independently
of runtime stdlib discovery. If the owning project's Ruby executable or version
cannot be detected, use the bundled Ruby 3.0 core stubs as a conservative
fallback and skip only runtime-dependent stdlib modules. Missing runtimes must
never turn universal core constants such as `Thread` into false unresolved
diagnostics.
Runtime stdlib discovery must invoke only the owning project's exact selected
runtime executable, with its exact Java home and without inherited Ruby,
RubyGems, or Bundler environment overrides. Never fall back to the server's
`PATH`, guess runtime homes from a version string, or rediscover bundled stubs as
runtime stdlib. A runtime path may replace only an existing `Stdlib` source at
the same canonical path; a collision with `Stub`, `Project`, `Excluded`,
`Signature`, `External`, or `Gem` ownership is an invariant violation.
Identical runtime load-path probes are one bounded server-owned process-local
single-flight product keyed by the canonical executable's byte length and
modified time plus canonical Java home. Verify executable identity before and
after the producer, canonicalize every returned path, and keep the producer
independent of the initiating waiter. Each isolated project still binds the
result through its own stdlib lifecycle and engine.
Reusable gem products must be seeded from a clean core/runtime engine, never by
cloning the owning project engine after core facts were added to it. An editor
may open or change a project document before cold indexing reaches core setup;
those project, excluded, or previously bound gem facts must not enter the
dependency seed or its semantic fingerprint. Install the shared core template
additively when live project facts already exist, while retaining a separate
clean template clone for the dependency seed. Add JRuby runtime implementation
inputs to both engines through the ordinary file-owned lifecycle, then fail
loudly if the reusable seed contains `Project`, `Excluded`, or `Gem` sources.
Persistent gem-product identity must include the build-generated SHA-256 of
every semantic fact-producer source, in addition to its explicit product and
payload schemas, parser/dependency identity, semantic seed, runtime provider,
locked closure, and exact source content. Keep `build.rs`'s producer input set
aligned with `ruby-analysis`, root fact composition, and JRuby import/catalog
semantics. A semantic producer change must reserve and publish a new product;
an older payload must never remain selectable merely because the package
version and manually maintained schema did not change.
Explicit `rubyVersion` configuration wins over automatic detection. RVM,
rbenv, and asdf `.ruby-version` markers must retain implementation prefixes
such as `jruby-` and `truffleruby-`; use those markers to select the matching
runtime executable and map its compatibility version to bundled core stubs.
JRuby support is a built-in runtime concern, not a framework extension or
editor concern. Compose the matching MRI compatibility stubs with a versioned
JRuby delta that can add, override, mark unavailable, or mask declarations.
For implementation navigation of JRuby's Ruby-authored runtime APIs, verify the
selected `jruby.jar` checksum and materialize only the bounded
`jruby/java/core_ext/{kernel,module,object}.rb` allowlist into the isolated user
cache. Index those files as runtime/stdlib implementation source so they
outrank compatibility stubs and signatures; never extract the archive broadly
or treat runtime source as project-owned.
Each isolated project owns its exact JDK/JAR classpath and generated Java proxy
facts. Bounded static classfile metadata may produce ordinary external
signature facts; never execute artifacts, merge project catalogs, or make
decompiled method bodies semantic truth.
Checksum-keyed Java artifact products are independent and may resolve in
parallel only inside the process resource governor's owned Rayon pool. Collect
the indexed artifact vector in its original order, then compose the
project-specific catalog sequentially so the first classpath definition wins
and duplicate provenance remains deterministic. Never parallelize the
classpath-precedence write itself or admit artifact work outside the owning
CPU, memory, I/O, and task lease.
An unconditional zero-retention single-flight around every persistent Java
artifact lookup was measured and rejected on the two-project JRuby `goshposh`
corpus: 1,074 lookups produced 1,074 independent flights and zero joins, while
median CPU, readiness, and RSS regressed. Do not restore that wrapper merely
because the keys are immutable. First prove concurrent identical product keys,
or remove the measured sequential decode/composition cost with a bounded design
that still preserves project catalog order and provenance.
The accepted sequential-reuse design keeps a server-owned cache of at most 256
exact Java artifact products and 256 MiB of estimated deep metadata. Archive
products and project declarations share immutable `Arc<ClassFile>` values;
project-specific paths, classpath ordering, duplicate winners, providers,
facts, and engines remain separate. On the two-project JRuby `goshposh` corpus
it retained 168 identities, reused all 190 repeated lookups, halved persistent
reads, and improved median wall, CPU, readiness, and RSS with exact semantics.
Keep capacity and deep-weight eviction tests, report reuse as a process-wide
counter, and never replace this per-artifact sharing with a composed catalog or
raw artifact retention.
Classpath discovery establishes each artifact checksum from one bounded,
metadata-stable byte buffer. Inspect a JAR's manifest from that same buffer;
do not reread the complete archive immediately after hashing it. Drop the
buffer after discovery rather than retaining raw package-manager artifacts in
Ruby Fast LSP's cache.
Identical canonical classpath files may share one process-local product keyed
by canonical path, byte length, modification time, and manifest parse limit.
The product retains only SHA-256 and bounded manifest `Class-Path` entries under
the fixed entry/estimated-weight limits; never retain raw JAR/JMOD/source bytes.
Every project consumer must revalidate current metadata, apply its own file and
total-byte limits, and compose its own ordered classpath and catalog. Cache hits
must never share duplicate winners, imports, provenance, or semantic ownership.
Bounded CFR implementation navigation must keep its fixed JVM heap, direct
memory, metaspace, code-cache, compressed-class-space, process-count, output,
and wall-clock limits. Its 256 MiB resident-memory ceiling is enforced through
native child RSS inspection on macOS, Linux, and Windows; inspection failure,
overage, or timeout must kill and reap the child before returning an isolated
decompiler error. Keep the limit aligned with the owning JRuby work claim and
change the decompilation cache options identity whenever a JVM bound changes.
Cold project indexing derives JRuby static-navigation plans from the ordinary
file pass's existing Prism tree and materializes those exact inputs before its
deferred resolution. Do not restore a separate project-wide Java preflight
read/parse. Keep the catalog-aware source prefilter complete for every supported
Java DSL/proxy form, and route interactive edits through the same
materialization lifecycle.

Full-document formatting is available through opt-in RuboCop or Standard
integration. It consumes the current unsaved buffer over stdin, uses RuboCop's
safe `--autocorrect` or Standard's `--fix`, and returns one UTF-16-correct
full-document edit only when output changes. Formatter selection and command
argv are independent from lint diagnostics. Startup failures, abnormal exits,
timeouts, invalid output, and unsafe empty output produce no edit and do not
mutate analysis state.

ERB analysis uses `ruby_analysis::indexer::mask_erb`. It replaces host-language
bytes one-for-one, preserves CR/LF and complete Ruby tag bodies, and therefore
keeps every Prism byte range in the original template coordinate space. Never
parse raw `.erb` content or compact extracted snippets in an LSP feature: use a
`RubyDocument`'s analysis content/parse result while converting ranges through
the original document. Host positions must not receive Ruby completion, and
Ruby formatter/linter integrations must not edit templates.

The VS Code adapter owns complementary ERB host-language behavior through
`editors/vscode/vsix/erb_html.js`. Its HTML projection preserves original
UTF-16 length, retains host markup, and masks every complete or unclosed ERB
region before calling `vscode-html-languageservice`. Keep this separate from
the Rust byte-offset projection: the server owns Ruby semantics, while the
editor adapter owns HTML UX. Do not delegate whole-document formatting or other
edits unless a range-safe merge policy proves embedded Ruby cannot be
overwritten. HTML diagnostics require their own false-positive and lifecycle
policy before publication.

Rails controller actions expose `ruby-fast-lsp.rails.openView` code lenses
through the ordinary extension response contract. The Rails guest owns only
controller/action discovery; the VS Code adapter validates workspace-safe
conventional view candidates and performs the editor open operation. Do not put
filesystem or editor-command execution into the guest or semantic engine.

Common Ruby source kinds are canonicalized in
`editors/vscode/vsix/ruby_file_kinds.json`. The VS Code manifest and watcher
patterns consume or are tested against that list, and the server discovery test
requires its extension/filename tables to match. Update the canonical JSON,
Rust policy, manifest, watcher tests, and packaged smoke together; do not add a
file kind to only one surface. `.erb`, `.rhtml`, and `.rhtm` all use embedded
Ruby mapping and must retain formatter/linter safeguards.

Project source ownership is selected only by `ProjectFilePolicy` plus the
indexer's explicit `SourceKind`; path heuristics outside that policy are
forbidden. Default-external directory components are `vendor`, `.bundle`,
`.ruby-lsp`, `.ruby-fast-lsp`, `node_modules`, `tmp`, `log`, and `coverage`.
`includedPatterns` may opt them in, `excludedPatterns` always wins, and `.git`
never participates. Opted-in/generated ordinary files are workspace-owned;
Gem/Stdlib/Stub files remain navigation inputs but are not editable, diagnostic,
or workspace-symbol sources and are hidden from the project-only namespace-tree
projection. Policy-excluded workspace files use `SourceKind::Excluded`: opening
them provides interactive facts/references without promoting them to editable
project truth, `didChange` must preserve that kind, and `didClose` must remove
their interactive-only facts. Closed-file watcher
create/change/delete events must use the same policy and the normal per-file
engine replacement lifecycle. Open buffers remain under didOpen/didChange/didClose.

External gem, stdlib, stub, and signature documents retain the isolated project
context that produced their navigation location. Subsequent requests for that
external URI must use the originating project's engine even though the path is
outside the project root. Without retained provenance, an exact dependency path
may use a project engine only when exactly one engine owns it. Ambiguous or
unknown external files stay diagnostic-free `SourceKind::Excluded` interactive
documents in the orphan engine; never merge project engines or promote an
external file to project truth. `didClose` releases retained provenance.

Native/generated declarations use existing semantic write paths. Ruby and RBI
stubs are ordinary indexed Ruby. Project `sig/**/*.rbs` files, plus additional
`.rbs` paths selected by `includedPatterns`, are converted in
`ruby-analysis::indexer::index_rbs` into ordinary symbols, methods, graph edges,
signature metadata, and RBS-provenance type facts, then enter the engine through
per-file replacement as `SourceKind::Signature`. Signature files are
non-editable and diagnostic-free. Engine method/constant navigation must prefer
matching Ruby implementations while signature help and missing implementation
types may use the RBS overlay. Watched RBS create/change/delete and parse failure
must replace or clear facts deterministically. Framework DSL/runtime-generated
APIs must continue through public extension patches and optional bounded
reindex requests; never add runtime reflection, decompiler-derived semantic
truth, or a second semantic store. Built-in runtime providers such as JRuby may
read bounded static declaration metadata, but must project it through the same
validated, file-owned fact lifecycle rather than a privileged semantic store.

Ruby source may begin with a shebang. `ruby-prism` 1.4.0's comment iterator can
segfault on raw leading `#!` input in optimized builds. All server/indexer Prism
parses and `SourceDocument` comment parses must use the offset-preserving
`ruby_analysis::indexer::mask_shebang` projection (`#!` -> `##`); never parse a
raw shebang through the comment iterator. The original document remains the
source for content and LSP position conversion.

Distribution versions are checked by `editors/check_package_versions.js` and
must match the root Cargo package across the VSIX, npm CLI, platform packages,
optional dependencies, and VSIX lockfile. VSIX creation and npm publishing fail
before packaging when versions drift. The npm install smoke test packs the local
CLI and current-platform package into a clean temporary project and proves the
installed wrapper can complete a real LSP initialize handshake.

Current-platform VSIX packaging must run `editors/scripts/smoke_vsix.js` on the
produced archive before moving it to `target/`. The smoke test extracts the
actual VSIX, executes its packaged platform binary, initializes it with the
bundled RSpec, Rails, Minitest, Sinatra, and Cucumber package paths from that
same extraction, and requires all extension statuses to be `loaded`. It clears developer extension-path
environment variables so a local package cannot mask a missing, invalid, or
checksum-broken bundled copy.

Current-platform VSIX packaging must rebuild the target-specific native binary
from the current checkout. Never reuse an existing native target artifact:
`cargo build --release` writes a different path and an old target-specific
binary can otherwise be silently packaged and installed. Only explicit
`--skip-builds` workflows may reuse the binaries already staged in the VSIX.

Wasm extensions are bounded by payload, memory, fuel, and wall-clock limits.
Each loaded extension owns one cancellable Wasmtime epoch ticker; every guest
call boundary resets its fuel and 500 ms epoch deadline, including allocation
and deallocation exports. Deadline traps disable only that extension and appear
as `slow` through `ruby-fast-lsp/extensions/status`.
The mruby shim transfers returned output ownership to the host. When the host
deallocates that output, the shim must clear its retained pointer and length so
the next guest call cannot free the same allocation again.

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
Read each discovered Wasm artifact once per discovery generation and reuse those
exact bytes for its fingerprint, manifest checksum, and compilation identity.
Reject source artifacts larger than 64 MiB before allocation and bound the
actual read against concurrent file growth. Compiled-Wasm persistent envelopes
must reject declared logical payloads larger than 64 MiB before allocation or
decompression.
Compiled Wasmtime modules are process-neutral derived products: key them by the
source digest and Wasmtime's exact target/compiler/config compatibility hash,
then persist only the host-produced serialized bytes inside the private bounded
derived-product cache. Before unsafe Wasmtime deserialization, validate the
cache envelope checksum, embedded source identity, compiler identity, artifact
length, and artifact checksum. A rejected native artifact must be removed under
the exact product lock and rebuilt from the already validated Wasm source; cache
failure must never prevent loading an otherwise valid extension. Each loaded
extension owns one ticker; its project guests share only that immutable compiled
module/ticker and retain independent stores, memories, limits, and mutable state.

Project-local extension discovery is fail-closed on workspace trust. Trusted
roots may contribute manifest packages from `.ruby-fast-lsp/extensions/*` and
`ruby_fast_lsp/**`; untrusted roots contribute none. Precedence is configured
or bundled packages, then project-local packages, then environment/dev paths,
with explicit packages before directory discovery and filesystem path as the
deterministic final tie-break across multi-root workspaces.

Manifest `[watching]` globs require the `watching` capability and must be valid
workspace-relative patterns without parent traversal. Supporting clients receive
a dynamically refreshed, sorted registration. Incoming file changes are scoped
to the deepest workspace root and pass through one server-owned 100 ms debounce
generation. The newest generation retains only the final event per URI in
deterministic order; older timers cannot process a partial filesystem state.
The normalized batch is matched per extension and delivered through bounded
`files.changed` events. Gemfile/lockfile, auto-runtime marker, trusted
project-extension, and owning JRuby classpath changes create at most one
replacement generation per affected project; ordinary closed source changes
retain the per-file replacement lifecycle. Watch callbacks
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
`process.completed` may return bounded `reindex_files` entries scoped to the
workspace roots related to the triggering event. The host rejects absolute or
traversing paths, deduplicates valid file URIs, and routes them through ordinary
watched-file reindexing. Runtime callbacks never return semantic patches
directly; cached runtime knowledge becomes facts only when normal call hooks run
again through validation and per-file engine replacement.

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

`extensions/rails-ruby` retains its stable package ID but is a Rust-authored
Wasm guest and must remain a normal consumer of the typed public guest SDK and
extension patch vocabulary. Its initial
static Active Record contract recognizes `belongs_to`, `has_one`, and
`has_many`, emitting generated public reader/writer methods, structured return
types, and exact references to conventionally inferred target classes. Active
Record callbacks and custom `validate` symbols emit exact method references;
the extension never performs method lookup itself. Route declarations use
public frame arguments, generated method facts, and exact references for
controller/action navigation; helper availability currently follows
`ApplicationController` inheritance. Active Job `perform_later`/`perform_now`
calls on conventional `*Job` constant receivers emit exact references to the
job's instance `perform` method. Model DSL facts inside concern `included`
blocks remain attached to the concern and flow through the existing engine
mixin/MRO graph; do not duplicate include or method lookup policy in the Rails
guest. Existing indexer/engine concern handling maps `class_methods` through a
generated `ClassMethods` module and the simulator owns its semantic coverage. The
deterministic WAT fixture and actual Rust-Wasm black-box tests must both prove
navigation, hover/type behavior, and stale-fact removal after edits. Keep Rails
inflection and DSL policy in this extension; do not add association names or
framework-specific resolution to `ruby-analysis` or the server indexer.

`extensions/minitest-ruby` retains its stable package ID but is a Rust-authored
Wasm guest using the public typed guest SDK. It contributes synthetic
`def test_*`, Rails-style `test "…"`, and Minitest::Spec group/example document
symbols plus Run and Debug code lenses, while core indexing remains the owner
of ordinary class/method symbols. Its spec execution contexts use source-scoped
hidden subclasses of `Minitest::Spec`: nested groups inherit, siblings remain
isolated, group `def` methods plus `let`/`subject` belong to group instances,
and example/hook/helper blocks preserve lexical/local scope while using the
group instance receiver. Applicability requires a complete owning-project lock
with Minitest `>= 5, < 7`. Keep this policy in the guest. VS Code owns terminal
argv shaping, workspace runner selection, structured process execution, and
`rdbg` launch configuration.
RSpec and Minitest debug lenses must start a debugger; a notification-only
placeholder is not a completed debug workflow.

Extension call arguments expose literal positional values plus flattened
keyword pairs. `Argument.keyword` carries the keyword name and its exact range;
`Argument.range` remains the value range. The field is optional for ABI v1
compatibility. Keep keyword extraction generic in `src/extensions`; option
meaning such as Rails `class_name` or `polymorphic` belongs in the consuming
guest. Unsupported/dynamic values must remain explicit rather than being
coerced into guessed strings.

Large, immutable project metadata may be delivered once at activation rather
than repeated on every call. Manifest `[indexing].project_context =
"activation"` opts into this mode: `lifecycle.activate` carries the complete
owning `ProjectContext`, while subsequent call payloads omit it. The host must
still retain and use the complete context for applicability and patch
validation. Packages that omit the field retain ABI-v1 per-call delivery.

An active lexical extension frame carries its owning extension IDs in
`ResolvedCall.frame_extension_ids`. Implicit/self calls inside that frame are
dispatched only to its owners; another extension may participate only through
an explicit non-self receiver. This host-derived provenance prevents common DSL
names such as `describe`, `before`, and `include` from creating cross-framework
facts when multiple supported testing gems are locked in one project.

Extension status telemetry is bounded and low-cardinality. It records call
classes, failures/traps/resource limits, rejection/conflict counts, emitted
patch families, total/max guest time, and per-project Wasm instance creation.
Count a disablement only on the first healthy-to-disabled transition so
concurrent in-flight failures cannot inflate evidence or overwrite the first
error. Official load stress must exercise every bundled guest, overlapping
framework applicability, unsupported versions, and isolated projects.

Manifest `[indexing].frame_call_names` declares lexical DSL frames separately
from guest handler `call_names`. Loaded frame calls are tracked even when the
guest has no handler for the frame itself, and `ResolvedCall.arguments`
preserves literal/keyword values plus ranges for nested handlers. Frame names
must be valid Ruby method names and are part of deterministic manifest reload
identity. Keep frame tracking framework-neutral; a guest must verify its own
root frame before interpreting nested calls.

Frame tracking does not establish the block's semantic execution context.
Ruby analysis must independently represent lexical constant scope, implicit
receiver, method-definition owner, and closure/local scope. RSpec example
groups, Sinatra/Cucumber-style execution blocks, and Ruby evaluation APIs can
change the receiver or definition owner while preserving lexical lookup. The
required framework-neutral execution-context contract and acceptance matrix are
specified in `extensions/README.md`; treat that work as blocking extension
architecture completeness and broad framework expansion. Do not encode another
framework-name list in `ruby-analysis` or reuse `current_namespace` as all four
contexts.

`extensions/sinatra-rust` is the bundled Rust/Wasm proof that exact existing
namespace targets do not require an RSpec-style generated owner. Sinatra route,
filter, and error blocks preserve lexical/local scope while switching the
implicit receiver to the application instance. `helpers do` uses the
application singleton as `self` and the application instance as the `def`
owner; constant helper arguments emit ordinary instance mixin patches. Classic
calls target `Sinatra::Application`, modular calls target the current
`Sinatra::Base` subclass, and applicability requires locked Sinatra `>= 3, < 5`.
Keep this policy in the guest. The host validator may accept namespace-only
execution contexts, but generated targets must still be declared in the same
patch.

`extensions/cucumber-rust` models Cucumber-Ruby's per-scenario World as a
project-scoped hidden owner. Step and scenario-hook blocks use that owner as
their implicit receiver while preserving lexical constants, closure locals,
and the source Ruby `def` owner. `World(SomeModule)` applies instance mixins to
the same owner across files; `World { factory }` must not receive World scope.
The guest supports locked Cucumber `>= 9, < 12`. Keep English DSL names,
top-level `Object` semantic targets, and all World policy in the guest.

Reusable execution templates are not Ruby mixins. Extensions connect a
template receiver to each concrete application through
`ConnectExecutionContextPatch`; fact conversion emits
`GraphEdgeKind::ExecutionContextApplication`. Engine method lookup searches
the template's ordinary chain first and then searches every application chain
independently. Never add these edges to Ruby MRO or select one application by
file/indexing order. Connection facts are file-owned and disappear through
ordinary replacement.

`ScopeTracker` owns the framework-neutral execution-context stack used by
static block forms of `class_eval`, `module_eval`, `class_exec`, `module_exec`,
`instance_eval`, `instance_exec`, `define_method`, and
`define_singleton_method`, as well as validated extension contexts. A frame
overrides implicit receiver and method-definition owner without replacing
lexical constant or closure/local scope. Ordinary nested blocks preserve the
frame; a real nested class/module or method body suspends it. A `def` declared
by an eval/extension context therefore receives that context's owner, but its
body runs with the declared method's instance/singleton receiver through a
method-runtime frame. Never let the eval block receiver leak into that method
body.

For block-form dynamic definitions, the block is the eventual method body:
`define_method` uses the target instance (or the target singleton when invoked
inside `class << self`) and `define_singleton_method` uses the target singleton,
while lexical constants, captured locals, and nested Ruby `def` ownership stay
tied to the source context. Static `send`/`__send__` and `const_get` receiver
chains follow the same rule. String-eval forms are an explicit unsupported
boundary and must never be parsed as block-form facts. Keep all of these rules
in `ruby-analysis`; framework guests only declare framework execution
contexts.

Generated semantic owners are represented by `GeneratedOwnerId` wrapped in a
reserved, non-Ruby `RubyConstant` sentinel. The sentinel is collision-proof
against `RubyConstant::new`, remains the same compact size as `Ustr`, and is
detected through `FullyQualifiedName::has_generated_owner`. Generated owners
may participate in graph/MRO/method/reference semantics, but constant
completion, workspace symbols, namespace trees, and rename must filter them.
Never replace this with a valid-looking synthetic Ruby constant or display the
reserved identity as user code.

`DefineMethodPatch` metadata is semantic, not decorative. The extension boundary
validates method/namespace/type/range/parameter payloads before conversion.
`file_processor` must preserve declared visibility, signature labels, and an
extension-provenance `TypeFact` for declared returns. During the same file pass,
the extension host mirrors the method identity/visibility and return type into
the collector's local facts so later expressions can infer it without a second
AST traversal. Final facts still enter the engine only through per-file
`replace_facts`; edit/reindex removes stale extension methods and types.
An extension may declare `return_type_source = Block` instead of a concrete
return type. The framework-neutral collector infers the call block's value in
the active lexical/local/execution context, mirrors it for same-pass receiver
inference, and persists it as an extension-provenance method-return fact. A
patch must never provide both a concrete return type and a derived source.

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
reference to a namespace, value constant, or method. The guest boundary
validates the target and range and conflicts patches by source range, rejecting
incompatible targets deterministically. Namespace/constant targets become
normal resolved candidates; method targets become diagnostic-free method
candidates and still pass through engine-owned MRO, visibility, ambiguity, and
reference resolution. Engine replacement stores them with parser candidates,
so references/highlights use existing query policy and edits remove stale
generated references. `AnalysisQuery` owns exact-target definition lookup at a
reference range and returns no result for conflicting targets; the LSP adapter
only converts its definition ranges to locations. Do not return LSP locations
or write directly to the engine reference store.

`SetSuperclassPatch` declares inheritance only for a class generated by a
matching `DefineNamespacePatch` in the same guest callback. The extension host
validates namespace/target/range/provenance, conflicts by generated class
identity, and rejects competing parents deterministically. Fact conversion
emits ordinary resolved or unresolved `Superclass` graph edges (plus singleton
inheritance when immediately resolvable), so engine MRO and hierarchy policy
remain single-sourced. Extensions must not override parser-owned inheritance.

Generated semantic owner identity has two explicit scopes. Source-scoped owners
combine extension ID, document URI, and local frame ID; project-scoped owners
combine extension ID, owning project URI, and logical local ID so a relationship
can cross files without crossing Gemfile-owned projects. `ApplyMixinPatch` may
target such an owner exactly through `mixin_target`; it must use exactly one of
that target or an ordinary Ruby `mixin` namespace. Project-scoped declarations
and targets require `ProjectContext` and fail closed when it is absent. Keep
these rules framework-neutral and preserve normal per-file fact replacement.

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
- Current implementation: `AnalysisEngine::replace_facts` records a
  range/order-independent semantic export fingerprint over declarations,
  method signatures/visibility, exported types, and graph relationships.
  `ProcessResult::semantic_change` distinguishes initial indexing, body-only
  edits, and exported API changes even though collection uses an intermediate
  direct-fact seed. During `didChange`, body-only edits publish only the current
  file; exported changes reprocess and publish at most eight deterministically
  sorted, project-owned open documents. Do not raise that bound or add closed
  workspace files to the typing path without a measured lifecycle benchmark.
- Shared symbol/type subject indexes are ordered by `SourceFileId` and the
  existing exact per-fact range key. Replacement-only stores remove one file,
  stable-sort only that file's appended tail, then rotate the complete file
  group into its binary-searched position. Do not restore a full shared-bucket
  sort. `TypeStore::add` is append-only and may create a non-file-ordered
  prefix; once that API is used, any later replacement must use the explicit
  full-sort fallback rather than assuming the splice precondition.
- Fact collection that needs known method-return types uses the borrowed
  `TypeStore::known_method_return_types` domain view. It preserves fact-arena
  order and filters unknown/unrelated facts before the collector clones the
  small domain values it owns. Do not replace it with `all_facts` expansion or
  a retained per-file lookup. Both an incrementally maintained borrowed
  `HashMap` and a compact FQN-sorted vector improved warm wall time by about
  3%, but raised median peak RSS by 11.0% and 22.3% respectively because their
  lifetime overlaps the full collector traversal. Both were measured, rejected,
  and removed; keep same-file return context short-lived unless a new profile
  proves a different ownership lifetime stays below the fixed RSS ceiling.
- Disabling `TypeTracker`'s discarded per-statement variable snapshots only for
  `FactCollector` return inference was also measured and removed. It preserved
  the exact semantic manifest and fingerprints, but the profiled target was
  only 0.41% inclusive and the controlled three-pair `goshposh` A/B produced no
  measurable gain: median wall, user CPU, active semantic readiness, and
  dependency navigation all regressed slightly. Do not add a special
  return-only snapshot mode without a new profile showing a materially larger
  target; evidence is in
  `support/performance/type-tracker-discarded-snapshots-rejection-2026-08-01.json`.
- During AST traversal, `FactCollector` and `VariableScopes` maintain one
  synchronized active lexical scope. Collector-local receiver type inference
  starts from that scope and lets `get_type_at_position` walk capturable block
  parents and stop at hard boundaries; it must not rescan every scope and
  variable location to rediscover traversal context. Cursor-driven completion,
  hover, rename, and diagnostic queries do not own that cursor and must retain
  their position-based scope lookup.
- Exact lexical, source-ordered `VariableScopes` facts are the sole authority
  for a local-variable receiver during fact collection. If that lookup has no
  defensible type, keep the receiver unknown. Never restore the removed
  whole-file text scan or nested Prism parse fallback: it crossed hard method
  boundaries, borrowed later same-named assignments, and created tens of
  thousands of false method-reference candidates on `goshposh`. The corrected
  semantic-result fingerprints and controlled A/B are recorded in
  `support/performance/fact-collector-source-ordered-local-receivers-2026-08-01.json`.
- Rejected August 1 2026 extension-call experiment: sharing the
  `tracked_call_names` prefilter between patch dispatch and enclosing-frame
  classification reduced warm wall time but failed the multi-root memory gate.
  Holding one registry snapshot raised median RSS 24.7%; preserving short-lived
  registry reads still raised median RSS 5.1%, slightly regressed CPU, and
  exceeded the fixed 1.777 GB ceiling in one controlled run. Both shapes and
  their temporary probe were removed. Do not merge these extension operations
  merely to eliminate the duplicate name lookup; evidence is in
  `support/performance/extension-call-classification-rejection-2026-08-01.json`.
- Exact owner/name resolution uses MethodStore's crate-private borrowed
  effective-fact selector. The already ordered bucket applies `Absent` and
  `Unavailable` precedence, collapses exact adjacent duplicates, and expands
  only one unique winner; distinct effective facts remain ambiguous. Keep MRO,
  execution-context applications, `method_missing`, diagnostics, and the public
  lookup result in engine resolution. Do not restore expanded-vector
  sort/dedup on this hot path or expose stored facts/arena IDs publicly.
- Per-resolution method-chain caching returns the stored interned owner IDs as
  a borrowed slice. A miss still constructs one chain through the sole
  engine-owned `method_lookup_chain` and translates only owners that can exist
  in the ID-keyed graph/method stores; hits must not clone the vector, rebuild
  FQNs, or probe each owner merely to release the cache borrow before recursive
  execution-context lookup. Keep the borrow scoped before recursion and do not
  replace it with a second MRO cache. The test-only `NameRegistry` lookup
  counter is measurement instrumentation and must remain absent from production
  builds.
- Rejected August 1 2026 experiment: traversing MRO directly through graph
  `FqnId` adjacency first changed complete semantic-result fingerprints because
  edge-only endpoint entries were mistaken for declared namespaces. The focused
  `edge_only_graph_entries_do_not_promote_missing_namespaces` regression now
  preserves that boundary. The corrected ID traversal still regressed the exact
  warm `goshposh` wall, CPU, and readiness medians, so it was removed. Do not
  restore that shape without a new symbolized profile, explicit graph-node
  definition semantics, and a three-run production improvement.

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

## Production Analysis Invariants

- Completed shared core-engine templates use a weighted single-flight cache
  bounded to eight entries and 128 MiB of engine-estimated heap. Completed gem
  products are deliberately ephemeral. Do not make either cache unbounded or
  merge isolated project engines to obtain reuse; profiler schema 6 records
  retained product weights while aggregate project-engine RSS remains a
  measured production acceptance budget.
- Project-source collection uses cooperative Rayon pools sized to
  `max(1, cpu_lanes / top_level_tasks)` after atomic resource admission. Do not
  run that phase in the full shared pool or claim fewer lanes than nested Rayon
  can actually use. Concurrent cooperative pools must remain bounded by the one
  task/CPU/transient-memory/I/O governor.
- Flow inference stabilizes only the outermost lexical `while`/`until` loop.
  Nested loops receive one semantic pass per outer iteration so generated
  parsers cannot turn the configured iteration bound into exponential work.
- Anonymous rest parameters (`*` and `**`) are explicit method-parameter
  kinds. Preserve them through indexing, arity checks, and signature help;
  absence of a parameter name does not mean absence of rest acceptance.
- A constant receiver may be a typed value rather than a class/module. Resolve
  its declared value type through `AnalysisQuery::type_to_namespace` and use
  ordinary instance-method lookup. Zero-argument `freeze` preserves the
  receiver's literal type so frozen constant declarations seed that fact.
