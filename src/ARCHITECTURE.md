# Ruby Fast LSP Architecture

This document describes the architecture of the Ruby Fast LSP server. The codebase is organized into several key components, each with a distinct responsibility.

## High-Level Architecture

The Ruby Fast LSP server follows a modular architecture with clear separation of concerns:

```
crates/
└── ruby-analysis/  - Reusable core facts, engine, inference, and parser-to-facts indexer
src/
├── capabilities/   - LSP/editor adapters, snippets, trigger handling
├── indexer/        - Workspace discovery and fact collection orchestration
├── query/          - LSP protocol adapters over ruby-analysis::engine::AnalysisQuery
├── handlers/       - LSP request/notification routing
├── server.rs       - LSP server coordination
└── main.rs         - Application entry point
src/test/           - Test harnesses and integration tests
editors/
├── scripts/        - Shared editor packaging and publishing scripts
├── vscode/         - VS Code extension assets, packaging script, and VSIX stubs
└── npm/            - npm package manifests and platform package directories
```

### Core Philosophy

1. **Separation of Concerns**: Each module has a clear, focused responsibility
2. **Loose Coupling**: Components interact through well-defined interfaces
3. **Single Responsibility**: Each file handles one aspect of the system

## Component Responsibilities

### 1. Indexer (`src/indexer/`)

The Indexer is responsible for discovering Ruby files, parsing them, and feeding facts into `ruby-analysis::engine`.

- **Primary Responsibility**: Workspace scanning and per-file fact collection
- **Secondary Responsibility**: Coordinate gem, stdlib, and project indexing

#### Key Files:

- `coordinator.rs`: Orchestrates workspace indexing
- `file_processor.rs`: Parses one file and runs `FactCollector`
- `indexer_project.rs`: Discovers and indexes project files
- `indexer_gem.rs`: Discovers and indexes gem files
- `indexer_stdlib.rs`: Discovers and indexes stdlib files

#### Design Decisions:

- Storage is owned by `ruby-analysis::engine`
- `FactCollector` emits symbols, methods, graph facts, references, diagnostics, and variable scopes in one AST pass
- File discovery and parsing stay separate from engine query logic

### 2. Analyzer (`crates/ruby-analysis/src/indexer/`)

The Analyzer is responsible for understanding Ruby code structure using the Prism parser.

- **Primary Responsibility**: Provide AST visitors for different analysis tasks (indexing, references, symbols)
- **Secondary Responsibility**: Extract semantic information from Ruby source code

#### Key Files:

- `mod.rs`: Central module file and Identifier resolution
- `scope_tracker.rs`: Tracks current namespace and scope during traversal
- `visitors/`: A collection of specialized visitors for different LSP features

#### Design Decisions:

- Uses the Visitor pattern for efficient AST traversal
- Separates analysis logic from feature implementation (Capabilities)
- Stateless analysis: processes one document at a time

### 3. Capabilities (`src/capabilities/`)

Capabilities implement LSP/editor feature entry points by coordinating query
adapters, analysis APIs, and editor-specific behavior.

- **Primary Responsibility**: Implement LSP feature endpoints, trigger routing, snippets, and editor-only behavior
- **Secondary Responsibility**: Convert between LSP types and internal types

#### Key Files:

- `definition.rs`: Go-to-definition entry point
- `references.rs`: Find-references entry point
- `hover.rs`: Hover information entry point
- `completion/`: Code completion coordination
- `semantic_tokens.rs`: Syntax highlighting functionality
- `type_hierarchy.rs`: Superclass/Subclass navigation
- `inlay_hints.rs`: Inline type and parameter hints coordination

#### Design Decisions:

- Each capability is self-contained in its own module
- Capabilities stay thin; reusable semantic analysis belongs in `ruby-analysis`
- For engine-backed queries, capabilities delegate to the **Query Engine**
- Capabilities handle LSP-specific concerns (request/validation/shaping)

### 4. Query Engine (`src/query/`)

The Query Engine provides a unified service layer for querying the `AnalysisEngine`.

- **Primary Responsibility**: Consolidate business logic for engine-backed queries
- **Secondary Responsibility**: Provide composable helpers for complex resolution (e.g., method return types)

#### Key Files:

- `mod.rs`: Defines `EngineQuery` struct and entry points
- `definition.rs`: Unified definition lookups
- `references.rs`: Unified reference lookups
- `hover.rs`: Type and documentation lookups
- `types.rs`: Type inference helpers
- `method.rs`: Method resolution and dispatch logic
- `inlay_hints.rs`: Unified inlay hints and on-demand inference logic

#### Design Decisions:

- Consolidates all "index-aware" logic into one place
- Provides a stable API for capabilities to query project-wide information
- Enables complex "chained" queries through composable helpers

### Project Containers and Engine Isolation

The LSP lifecycle distinguishes editor workspace folders from Ruby projects.
A folder with a root Gemfile is one project; a folder without one expands to
its nearest nested Gemfile roots unless `indexing.projectRoots` explicitly
defines non-overlapping roots. `.git` is never a project marker. Discovery and
workspace-folder ownership live in `src/`, not `ruby-analysis`.

Each registered project owns an `Arc<RwLock<AnalysisEngine>>`. Document,
watcher, hierarchy, rename, completion, diagnostics, and extension queries use
the engine selected by longest project-root prefix. Files outside every project
use a separate orphan engine. Workspace-symbol search is the intentional
cross-project operation and aggregates already-shaped results deterministically;
semantic lookup never combines project engines. Cold gem and stdlib indexing
receive the owning engine explicitly because dependency paths commonly live
outside the project root or under external directories.

### Multi-Root Scheduling and Reusable Dependency Products

The server owns one bounded indexing scheduler and one typed status generation
per discovered Ruby project. A coordinator reports phase state upward; it does
not publish editor progress directly. Admission is exclusive per project, while
different projects may run concurrently within the scheduler's measured
resource limit. The active document's deepest owning project receives priority,
but already-ready engines remain independently queryable. Priority is bounded:
while admissible background work waits, one active/open-document admission may
bypass it, then the oldest admissible background project owns the next slot.
Same-project generations remain mutually exclusive throughout this fairness
policy.

The project scheduler controls generation admission; the process resource
governor controls every admitted cold-indexing phase. Its one fairness-aware
queue reserves top-level task count, CPU weight, conservative transient-memory
bytes, and I/O slots in one locked transition. It never acquires resource
dimensions separately, so a waiter cannot hold CPU while waiting for memory or
I/O. Impossible requests fail loudly instead of waiting forever. Releasing one
RAII lease returns the exact complete claim and wakes the queue. Active-project
intent is retained before work is enqueued and reprioritizes both the scheduler
and resource queue. Cancellation is exact while queued. After a
non-interruptible blocking phase begins, its lease stays live until that worker
really exits. Async external work holds the same RAII lease across the future:
normal completion, panic, pre-admission cancellation, and post-admission task
cancellation have distinct accounting, and dropping an admitted future releases
its entire claim exactly once.

Nested Rayon work reserves the complete server-owned CPU pool and executes only
inside that pool. Sequential work executes on a blocking worker with a
one-lane claim. The default six-lane ceiling leaves host capacity for the LSP
reactor and editor. The 512 MiB transient budget and two I/O slots are an
internal candidate selected against the recorded M0 envelope, not editor
settings or a final accepted default. Profiler-only flags may override all four
dimensions to record reproducible evidence before that default is accepted.
These reservations bound admitted temporary work. Completed shared core-engine
templates use a weighted single-flight cache capped at eight entries and
128 MiB of engine-estimated heap; in-flight consumers keep their immutable
value safely across eviction. Completed gem products retain no process-local
values after their concurrent consumers finish. Isolated project engines remain
the separately owned semantic truth and are not evicted or merged; their
aggregate estimated heap and final process RSS must stay under the goal's
measured acceptance budget. Profiler schema 6 reports both retained product
weights and core-template evictions.

Project-source collection is the bounded exception to full-pool Rayon
reservation. It uses cooperative pools whose exact lane count is
`max(1, cpu_lanes / top_level_tasks)`. Each pool is created only after its
task/CPU/transient-memory/I/O claim is atomically admitted, so all live
cooperative pools together cannot exceed the same process budget. Full-pool
phases still use the server-owned shared Rayon pool and cannot overlap a
cooperative claim that leaves insufficient lanes. This prevents one large
project scan from serializing every sibling while keeping nested `par_iter`
work honest about its usable width.

While the active project's navigation reservation is pending, its source pass
uses `max(1, cpu_lanes - 1)` lanes instead of the ordinary cooperative
partition. The remaining lane, task admission, transient-memory partition, and
I/O slot may overlap only independently immutable dependency discovery for that
same active project; project-parallel work for sibling roots remains blocked by
the navigation reservation. Discovery returns its exact resolver state to the
owning coordinator, which performs dependency-product binding through that
project's isolated engine after the source pass. No detached semantic engine or
second dependency catalog is created.

Ruby version detection, project discovery, Rayon fact collection, core/stdlib
construction, JRuby classpath/catalog and runtime/signature source
materialization, gem discovery/manifest preparation, checksum-keyed gem
product construction/binding, final engine resolution, and engine compaction
all pass through this governor rather than the async LSP reactor. Shared
automatic gem discovery keeps Bundler resolution and its global RubyGems
fallback inside one invocation of the exact selected runtime, returning an
explicit source marker so selection precedence remains unchanged without a
second process startup. A standalone project root without a `Gemfile` does not
run installed-gem discovery: it indexes project, bundled core, and available
stdlib inputs only. After project scanning, an explicit `includedGems` set may
deliberately schedule the sole unlocked active-runtime global-gem exception;
ordinary `require` calls do not. Shared
single-flight producers deliberately do not inherit one waiter's cancellation;
consumer binding does. Coordinator phase transitions, single-flight waiting,
and status publication stay async. New indexing phases must follow the same
split: immutable inputs and owned engine handles cross the worker boundary;
editor/LSP transport does not. Runtime installation scans and bounded version
probes, trusted extension watched-file child processes, and RuboCop/Standard
lint, correction, and formatting subprocesses also hold weighted async leases
for their complete lifetimes. Open, change, and save notifications serialize
semantic replacement per document, then run the complete parser/indexer pass on
a blocking worker under one open-document weighted lease. This includes
index-time Wasm extension calls without double admission. Weak per-URI async
locks preserve newest-version ownership while allowing unused document locks to
be reclaimed. Extension discovery/load/reload is serialized and runs on a
blocking worker under one background weighted lease. Registry replacement holds
the write lock only for the atomic swap; guest construction, activation, and old
guest deactivation happen outside it. Request-time document-symbol and code-lens
guest calls use open-document weighted leases and bypass admission when no
loaded extension implements that response surface. The server owns this one
configured registry; project coordinators adopt it instead of independently
reading and activating identical packages.

Each isolated project also retains the exact JRuby import provider produced by
its coordinator. Interactive document processing selects that provider through
the same longest-root/external-provenance ownership rules as the analysis
engine. Lazy Java signatures, verified source materialization, and bounded
decompiler work triggered by didOpen/didChange/didSave therefore remain inside
the notification's outer open-document lease without leaking a classpath across
projects or acquiring a nested lease. The redundant server-wide system-Ruby
probe was removed; runtime compatibility is resolved independently by each
project. CFR additionally runs with explicit heap/direct/metaspace/code-cache
bounds and a measured 256 MiB resident-memory ceiling. The server samples
native child RSS on macOS, Linux, and Windows and kills/reaps the JVM if
inspection fails, the ceiling is crossed, or the wall-clock timeout expires.

Cold project indexing does not run a second JRuby source scan. The ordinary
project file pass reads and parses each source once, derives the static Java
navigation plan from that existing Prism tree, and materializes its exact
signature/source inputs before the batch's deferred engine resolution. A cheap
source prefilter recognizes only supported Java DSL entry points, canonical
`Java::` proxies, and exact top-level packages present in the owning project's
catalog; ordinary Ruby files therefore avoid another AST traversal. Interactive
edits use the same plan and materialization path after their ordinary parse.
Java inputs remain file-owned facts in the isolated engine rather than a
parallel semantic store.

Retained-memory accounting still requires completion before resource governance
is considered production complete.
The old coordinator-wide Ruby load-path probe is not part of production
startup: gem and stdlib discovery already resolve their exact owning-runtime
inputs, and no semantic consumer used that duplicate side table.

Indexing status request/notification snapshots receive their global sequence
under one async publication lock. The VS Code adapter applies only strictly
newer complete snapshots and caches both aggregate and per-project state, so an
older delayed request cannot overwrite projects from a newer notification.
Generation, phase, aggregate scheduler state, readiness, cancellation, and
failure changes publish immediately. Same-phase counter-only updates share one
200 ms pending flush, and an immediate transition cancels that stale flush.
The editor renders accepted notifications directly; it does not echo each
notification into another status request.
The detailed picker joins server-reported runtime, JDK, and classpath identity
by exact project root. Its cache evidence is an explicit process-lifetime
snapshot of persistent gem products, persistent Java artifact products,
persistent compiled-Wasm products, gem single-flight joins, and classpath-file
single-flight hits/joins. Those counters remain process-wide because shared
immutable work can serve several isolated engines and cannot be defensibly
attributed from global counter deltas.
Language-client restart suspends the editor status session before transport
replacement, coalesces concurrent restart callers, and resets the accepted
sequence only after the old transport has stopped. Disposal permanently rejects
delayed notifications. This is required because each new server process begins
its authoritative sequence at one.
Every active-editor status request includes the document URI; the server maps
it to the deepest owning project and updates both priority owners even when the
document was already open.

Watched-file notifications pass through one 100 ms server-owned generation
gate. A newer batch invalidates the older waiter and overwrites each URI with
its final event before deterministic URI-order processing. Shutdown invalidates
the pending batch. Ordinary closed project/RBS files use exact per-file
replacement. Gemfile/lockfile, auto-runtime marker, trusted project-extension,
and owning JRuby classpath inputs create one scheduler-owned replacement
generation for the affected project, clearing its runtime/external semantic
state only after the prior generation releases project admission.

The shared core-engine clone is valid only while the requesting isolated engine
is empty. An open or changed document may add live project facts while a
coordinator waits for the shared producer. The coordinator rechecks under the
engine write lock: an empty engine receives the clone, while an engine with any
live facts receives core stubs additively through ordinary per-file
replacement. Never overwrite an engine merely because startup has not reached
project collection; that would erase document-ready navigation and potentially
unsaved content.

Cold Ruby project collection reads one immutable, generation-owned semantic
baseline captured after signatures and extension semantic seed facts are
installed but before any Ruby project file is collected. The coordinator
pre-registers every project path in deterministic order and removes stale
`Project` and `Excluded` exports from the snapshot. Demanded files, the active
navigation frontier, exhaustive batches, and JRuby catalog-sensitive replay all
read this same baseline while publishing their results only to the live
isolated engine. Navigation demand may change when a file becomes queryable; it
must never change the facts ultimately collected from identical source,
configuration, dependency products, and extension inputs.

Shared work must not imply shared semantic ownership. A reusable dependency
product has three distinct layers:

1. **Artifact identity** in `src/`: exact lockfile source identity, runtime and
   platform compatibility, analyzer/parser/schema version, extension/runtime
   provider fingerprint, and checksums for every source input.
2. **Project-neutral semantic template** in `ruby-analysis::engine`: immutable
   declarations and graph/type facts that cannot be inserted directly because
   their template file IDs are private.
3. **Project binding** in `src/indexer`: register the requesting project's exact
   source path/content/kind, instantiate every template with that engine's file
   ID, validate provenance and source precedence, then use the ordinary
   `AnalysisEngine::replace_facts` lifecycle.

`ProjectNeutralFileFactsTemplate` is the first engine primitive for this
boundary. It accepts only ranges owned by one template source and rejects
reference candidates, diagnostics, and execution contexts. Those facts depend
on project/query/extension state and cannot enter a generic external dependency
cache. Instantiation clones and rebinds every supported range, including
expression type subjects, before returning ordinary `FileFacts`.

Gem templates must be produced in a deterministic dependency-only engine seeded
by the exact runtime/core semantic input—not by whichever project happens to
win a race. The editor may open a project document before core setup, so the
coordinator retains a clean core template for the dependency seed even when it
must install the same core facts additively into a non-empty project engine.
JRuby runtime implementation inputs enter both engines through the ordinary
file-owned lifecycle; a production invariant rejects project, excluded, and
previously bound gem sources from the reusable seed. The product key includes
the exact selected gem identity, its declared dependency context, logical
source paths and checksums, the clean semantic seed, and the JRuby provider
identity only when the gem source is catalog-sensitive. Each consumer supplies
its own physical paths. Concurrent identical requests join one producer. The
gem product is currently an
**ephemeral flight**, not a completed-value memory cache: completion wakes all
overlapping consumers and then removes the template. A measured sequential
JRuby run retained 112 MB after the first project but could reuse only 3.3 MB
for the second project, with no material dependency-readiness improvement.
Completed retention was therefore rejected. Later sequential and fresh-process
reuse belongs in demand-loaded persistent storage that is content-addressed,
atomically published, checksum-verified, disk-bounded, and owned exclusively by
Ruby Fast LSP.

Cache acceptance requires semantic evidence, not a hit counter: cold and reused
definition/type/signature/graph queries must agree, navigation must resolve to
the consumer engine's exact source location, and unrelated project facts must
not affect the product. Cache lookup, validation, deserialization, rebinding,
insertion, retained memory, and eviction are all measured separately.

Extension discovery reads each Wasm file once and carries those exact immutable
bytes through discovery fingerprinting, manifest-checksum validation, and
compilation. The server-owned derived-product cache keys a serialized Wasmtime
module by the Wasm source digest plus Wasmtime's target/compiler/config
compatibility identity. Its nested envelope validates source identity, compiler
identity, lengths, and checksums before the host crosses Wasmtime's unsafe
deserialization boundary. Rejected native artifacts are invalidated under the
same cross-process product lock and rebuilt from validated Wasm; cache failures
fall back to ordinary compilation. Restored modules still preserve one engine
and one epoch ticker per loaded extension. Wasm source reads and compiled-Wasm
logical cache payloads each have a 64 MiB ceiling enforced before allocation or
decompression; the source reader also catches growth after metadata inspection.
Project guests share the loaded extension's immutable module and ticker but own
independent stores, memories, limits, and mutable state.

JRuby Java source archives retain their classpath-discovery checksum as the
content identity and an exact length/mtime pair as a cheap same-process
stability check. A project-local source resolver opens each archive lazily,
parses its central directory once behind a mutex, compares entry names through
central-directory metadata, and reads only the selected source entry. Parsed
archives are never shared across project semantic owners; reusable JAR/JMOD
class metadata belongs in checksum-keyed immutable products, while classpath
precedence, duplicate selection, imports, and engine insertion remain
project-owned.

Cold Java catalog preparation resolves independent checksum-keyed JAR/JMOD
products in the resource governor's owned Rayon pool. Indexed collection keeps
the exact classpath vector order; catalog composition is then sequential, so
duplicate classes retain first-entry-wins semantics and exact winner/shadowed
paths. The parallel phase owns the full CPU, transient-memory, I/O, and task
claim. It does not parallelize project-specific precedence or create a second
catalog store.

Classpath discovery reads each artifact into one bounded buffer while checking
length and modification time before and after the read. That buffer establishes
the checksum and, for JARs, is also the input for bounded manifest classpath
expansion. It is dropped immediately after artifact registration. This avoids
an unconditional second full-archive read without retaining package-manager
bytes or weakening content-drift detection.

The server owns a process-local classpath file-product cache capped at 4,096
entries and 16 MiB of estimated descriptor weight. Its key is the canonical
path, byte length, modification time, and fingerprint-only versus exact
manifest-entry limit. One synchronous single-flight producer derives SHA-256
and bounded manifest entries from the same metadata-stable buffer; completed
products retain no raw artifact bytes. Errors and panics wake waiters and remove
the entry so later requests retry. Every consuming project revalidates metadata
and reapplies its own per-file and total-byte limits before independently
composing classpath order, duplicate winners, Java catalog, imports,
provenance, and engine facts.

### 5. Server (`src/server.rs`)

The Server coordinates between LSP clients and the internal components.

- **Primary Responsibility**: Route LSP requests to appropriate components
- **Secondary Responsibility**: Manage server state (document cache, etc.)

#### Design Decisions:

- The server and root adapters own request-time LSP protocol conversion. The
  complete `ruby-analysis` crate is editor-independent and exposes domain
  positions, ranges, symbols, tokens, and source identities; no reusable
  analysis module imports or depends on `tower-lsp`.
- The server delegates actual implementation to capability modules
- The server maintains minimal state (mostly for coordination)

### 6. Inference (`crates/ruby-analysis/src/inference/`)

Inference derives types from local flow, method bodies, semantic lookup, and
RBS contracts. Its authoritative proof model and change protocol live beside
the implementation in the module-level Rustdoc at
`crates/ruby-analysis/src/inference/mod.rs`; this section describes how that
module participates in the complete system.

The semantic path is intentionally one-way:

```text
Prism traversal in ruby-analysis::indexer
        -> file-owned facts, candidates, flow evidence, return equations
        -> engine graph and immutable query context
        -> bounded inference/recursive solve
        -> engine-owned solved outcomes and diagnostics
        -> thin LSP and check-CLI projections
```

This shape preserves three independent concerns. The indexer owns Ruby syntax
and lexical traversal. Inference owns derivation, joins, narrowing, RBS
substitution, and bounded solving. The engine owns persistent project truth and
the only method/MRO/visibility/ambiguity policy. A new feature must not bypass
those boundaries by reparsing for one request, looking up methods locally, or
storing a second result for one consumer.

Inference queries accept `SourceFileId`, domain ranges, and UTF-8 byte offsets.
Root LSP adapters convert UTF-16 positions through the current `RubyDocument`
generation exactly once before calling reusable analysis. A recursive
architecture test rejects direct editor-protocol imports and manifest
dependencies anywhere in `ruby-analysis`.

Concrete results are proof-carrying. An incomplete union, receiver, lookup
chain, overload, recursive component, or type substitution remains an
explained Unknown. A proven outer collection may retain an unknown argument,
but that partial shape cannot prove a diagnostic requiring a concrete element.
Method existence is independent from return proof: exact navigation and
references may survive while hover and chained inference remain Unknown.

Universal runtime values such as `ARGV` are parsed from the embedded core
`constants.rbs` and installed through the ordinary file-owned
`SourceKind::Signature` lifecycle. The physical RBS file remains the navigation
target. Neither the collector nor an adapter synthesizes a type from a constant
name. RBS, YARD, runtime, and validated extension types all enter this same fact
and provenance lifecycle.

A nested call candidate retains the exact inner expression range that supplies
its receiver. Engine resolution first finalizes the inner call against the
complete graph, then admits the outer dispatch only when that outcome is proven
and its type agrees with the graph node's class/module kind. Reopened methods
and union returns use the same fail-closed path. Ruby source `initialize`
becomes callable singleton `new` only when its owner is proven to be a class;
its semantic return is the constructed instance. Explicit `self.new` remains
an ordinary method, and modules never acquire constructor semantics from
syntax alone.

The remaining reusable infrastructure seam is higher-order call solving. The
current analysis handles selected yields, block bodies, proc/lambda calls, and
receiver-generic RBS returns, but there is not yet one callable constraint model
that solves generic results from block input/output. Static symbol-to-proc forms
such as `items.map(&:to_s)` therefore remain a known false Unknown. That gap
belongs in `ruby-analysis::inference` as a general block/proc/generic
substitution rule, not as collection-method cases in LSP features or the
checker.

The reviewed acceptance contract is
`support/type_inference/scorecard.toml`. Historical accepted and rejected
measurements live under `support/performance/`; they are evidence, not a second
architecture document. Mandatory release and memory gates are summarized in
`AGENTS.md`.

### 7. Handlers (`src/handlers/`)

Handlers manage the routing of LSP requests and notifications.

- **Primary Responsibility**: Receive requests from the server and route them to capabilities
- **Secondary Responsibility**: Handle document lifecycle notifications (open, change, save)

### 8. Ruby Version (`src/indexer/version/`)

Ruby version detection and version-manager integration.

- **Key Types**: `RubyVersion`

## Key Workflows

### 1. Workspace Indexing

1. Client connects to the LSP server
2. Server initializes and receives workspace information
3. Server asks the indexer to index all Ruby files in the workspace
4. Indexer finds all Ruby files and processes each one:
   - Parse the file using Ruby Prism
   - Traverse the AST once to collect facts and candidates
   - Replace that file's facts in `AnalysisEngine`

### 2. Go to Definition

1. Client sends a "go to definition" request with a position
2. Server delegates to the definition capability (`src/capabilities/definition.rs`)
3. Definition capability:
   - Uses the analyzer to identify the identifier and local scope at the position
   - If not a local variable, delegates to the **Query Engine** (`src/query/definition.rs`)
4. Query Engine:
   - Uses `EngineQuery` to perform project-wide lookups in `AnalysisEngine` (handling inheritance, mixins, etc.)
   - Returns resolved locations
5. Capability returns the location(s) to the client

### 3. File Change Handling

1. Client edits a file and sends a "did change" notification
2. Server receives the notification and:
   - Updates its document cache
   - Asks the indexer to reindex the file
3. Indexer:
   - Parses the updated content
   - Replaces that file's facts in `AnalysisEngine`
   - Recomputes engine diagnostics
   - Compares an engine-owned semantic export fingerprint that excludes method
     bodies, source ranges, references, locals, and diagnostics
4. The LSP lifecycle adapter publishes the changed file immediately. A
   body-only edit stops there; an exported declaration/signature/type/graph
   change may reprocess at most eight sorted, project-owned open documents so
   active cross-file diagnostics refresh without project-wide typing fanout.
5. Cold indexing retains workspace diagnostic facts in the engine but projects
   them to the LSP client only for open documents. Closed-file diagnostics are
   available to agent/engine queries and are published if that file is opened.
6. Missing-method publication is conservative when graph resolution is
   incomplete: an unresolved superclass/mixin edge makes absence inconclusive.
   The reference candidate remains available, while diagnostic and signature
   policy stay single-sourced in the engine.
7. A call with a proven union receiver stores that canonical receiver behind
   its existing per-call candidate metadata. The engine resolves the receiver
   members as one fail-closed MRO/visibility group and materializes the call as
   a reference to every exact target only when all members resolve. Method
   existence is independent from return-type proof: an exact grouped call may
   retain an explained Unknown return without becoming an unresolved method.

## Component Interactions

### 3-Layer Architecture

The Ruby Fast LSP follows a clear 3-layer architecture:

1. **API Layer** (`server.rs`, `handlers/`): Handles LSP protocol, request validation, and routing.
2. **Service Layer** (`src/query/`, `src/capabilities/`): Implements business logic for LSP features. `EngineQuery` acts as the primary service interface for data lookups.
3. **Data Layer** (`ruby-analysis::engine`): Owns symbols, graph facts, references, diagnostics, and type facts.

### Analyzer, Query Engine, and Indexer Relationship

The separation between these components is crucial:

- **Analyzer**: Focuses on "what is this piece of code?" (local context)
- **Query Engine**: Focuses on "where is this in the project and how does it relate to other code?" (global context)
- **Indexer**: Focuses on file discovery, parsing, and feeding facts into the engine.

This separation allows:

1. Independent evolution of each component
2. Clearer testing boundaries
3. Better caching strategies (indexer can be persistent, analyzer is on-demand)

### Capability and Query Engine Relationship

Capabilities use the Query Engine as their primary data service:

1. Capabilities handle the AST traversal and identifying _what_ the user is interacting with.
2. They call the Query Engine to resolve _where_ that thing is defined or referenced across the workspace.
3. They translate the results back into LSP-specific formats.

## Future Extensions

The modular architecture facilitates extending the server with new capabilities:

1. Add a new capability module in `src/capabilities/`
2. Use existing services (Analyzer, Indexer) as needed
3. Wire it up in the server implementation
4. Update server capabilities in the initialize method

## Performance Considerations

- The Indexer builds an in-memory index for fast lookups
- Document changes trigger targeted reindexing
- Analysis is performed on-demand rather than eagerly
- `AnalysisEngine::replace_facts` records a deterministic per-file semantic
  export fingerprint and reports initial, body-only, or exported-API change.
- Shared symbol/type subject indexes retain deterministic `(SourceFileId,
  range...)` ordering without re-sorting an existing bucket for every file.
  Replacement-only stores stable-sort the appended file tail, binary-search
  its file position, and rotate the file group into place. A `TypeStore` that
  has used append-only `add` explicitly falls back to a full stable sort for
  later replacements because its prefix is not guaranteed to be file ordered.
- Method inference seeds each collector from the borrowed
  `TypeStore::known_method_return_types` domain view. The view preserves arena
  order and filters unrelated/unknown type facts before cloning, without
  exposing store IDs or moving lookup policy out of the engine.
- `TypeStore` interns each structurally equal `RubyType` once and retains
  compact deterministic IDs in stored facts, resolved-call outcomes, and
  concrete local-read evidence. Public engine and CLI/LSP domain boundaries
  expand those IDs back to values; interned IDs never escape as semantic
  identity.
- A stored expression subject is one four-byte tagged ID. Its high bit marks an
  expression and its payload reuses the fact's existing `TextRange`; expression
  facts therefore do not duplicate a range, enter the global subject interner,
  or allocate per-subject hash buckets. A 64-bit layout test pins
  `StoredTypeFact` at 24 bytes.
- Resolution drops pass-local caches before installing the owned compact
  resolved-call map. This avoids simultaneously retaining duplicate expanded
  outcome collections during the project-wide resolution peak.
- AST-time local receiver inference starts from the `VariableScopes` lexical
  cursor already synchronized by `FactCollector` traversal. The existing type
  lookup walks capturable block parents, respects hard method/class boundaries,
  and applies source-order assignments without an all-scope ownership scan.
  Request-time analysis queries receive the document's domain byte offset from
  the root adapter because reusable analysis does not own protocol-coordinate
  conversion. `ruby-analysis` has no `tower-lsp` dependency: its document,
  analyzer, indexer, rename, hover, symbol, token, YARD, engine, and inference
  surfaces use `SourcePosition`, `SourceRange`, `TextRange`, and domain enums.
  Root `src/` modules alone project those records into LSP positions, ranges,
  locations, symbol kinds, and semantic tokens.
- A failed lexical lookup remains unknown. Fact collection does not fall back
  to scanning source lines or reparsing assignment fragments: such a fallback
  cannot preserve Ruby lexical ownership or source order and can attribute a
  receiver to a same-named local from another method. This is both a semantic
  boundary and a performance boundary; all AST-time local receiver knowledge
  flows through `VariableScopes`.
- Final method-reference resolution asks MethodStore for a crate-private
  borrowed effective match in one exact owner/name bucket. Storage applies
  runtime availability precedence and exact duplicate collapse in its existing
  deterministic order; engine state expands only a unique winner, while
  resolution continues to own MRO, ambiguity, execution contexts,
  `method_missing`, and diagnostics.
- The resolution-local method-chain cache exposes its compact interned owner
  IDs as a borrowed slice. Cache misses still construct the chain through the
  sole engine-owned MRO function and translate its members once; repeated
  method names on the same receiver scan the same allocation and select exact
  method facts by owner ID. The borrow ends before recursive execution-context
  or `method_missing` lookup mutates the cache. A direct graph-ID MRO traversal
  is intentionally absent: its corrected form regressed the production replay,
  and its first form exposed that edge-only adjacency endpoints are not graph
  node declarations.
- `didChange` never performs project-wide affected-file propagation. Export
  changes refresh at most eight open project documents; body-only changes
  refresh only the edited document.
- Repeatable cold, edit, query-p95, and estimated-engine-memory budgets are
  defined in `AGENTS.md`, checked by the release profiler, and backed by exact
  machine-readable evidence under `support/performance/`.
- Loop flow stabilization is bounded in `ruby-analysis::inference`: only the
  outermost lexical loop repeats to the configured fixed-point limit, while
  nested loops receive one pass per outer iteration. This prevents exponential
  analysis of generated parsers without moving inference policy into the LSP.
- Constant-call collection distinguishes namespace constants from typed value
  constants. The collector asks the engine query API to map a value's
  `RubyType` to its namespace, then uses the same engine-owned instance-method
  resolution as every other receiver; zero-argument `freeze` preserves the
  literal receiver type during the direct-fact seed pass.
