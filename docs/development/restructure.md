# Restructure plan

Goal: a codebase that can be understood module by module. Each module has one
owner, a small public surface, and plain values at its boundary. No features are
removed. The target design is the `FileAnalysis` → `Project` → `View` model:
analysis produces one per-file value, a project owns a few components, and
features read through free functions over a view.

Delete this file when the last task is done. Git history keeps the record.

## How to work

- Each task is one commit (or a short series) pushed to `main` with the full
  workspace suite green, `cargo fmt --all -- --check` clean, and
  `python3 -B support/structure/check.py` passing.
- Tasks that touch indexing, inference, or scheduling also need a profiler
  comparison before and after. Results go under `target/`, not into the repo.
- Tick the box in the commit that finishes the task. If a task changes a
  contract, update the nearest guide in the same commit.
- Do one task at a time. If a task turns out larger than it looks, split it
  here first.

## Phase A: remove weight without changing the design

- [x] A1. Add a robustness harness over real Ruby sources. It checks for no
      panics, that incremental edits match a fresh index, that open order does
      not matter, and that clean code has no diagnostics.
- [x] A2. Move the handwritten lifecycle and schedule cases out of the simulator
      into `src/test/integration/`.
- [x] A3. Delete the simulator binary, its generator and oracle, the
      simulation build identity in `build.rs`, and the simulation guide and
      release step.
- [x] A4. Move `src/bin/*` and `utils/perf` into a `crates/devtools` crate so
      the server crate holds only the server.
- [x] A5. Delete dead code, APIs used only by tests, and unused debug
      endpoints.
- [x] A6. Add an `invariant!` macro and shorten the multi-line invariant
      messages without losing what broke, why it is a bug, and the fix.
- [x] A7. Replace scattered statistics plumbing with one stats registry.
- [x] A8. Remove `mod.rs` files that only re-export.

## Phase B: analysis data model (`crates/ruby-analysis`)

- [x] B1. Introduce `FileAnalysis` as the single per-file output. Keep it
      alongside `FileFacts`, `AnalysisIndex`, and `CollectedFile`, then remove
      those three.
  - [x] B1a. Move `engine::FileFacts` to `core::FileAnalysis`
        (`core/source/file_analysis.rs`) with the same fields, and leave
        `FileFacts` as a temporary alias. Switch the engine internals
        (lifecycle, fingerprint, template, codec).
  - [x] B1b. Make `AnalysisIndexer` and `index_rbs` return `FileAnalysis` and
        type the collector's direct facts as `FileAnalysis`. Delete
        `AnalysisIndex`, `file_analysis_facts_from_index`, and the RBS and
        stdlib field-copy blocks.
  - [x] B1c. Give the collector one `FileAnalysis`. `finish()` returns
        `FactCollectorOutput { analysis, flow_types, extension_patches,
        document }`. Delete `CollectedFile`. Add
        `FileAnalysis::replace_declarations`.
  - [x] B1d. Merge the two server assembly paths (`process_file` and the
        batch collection path) into one `compose_file_analysis`, with a
        profiler run before and after.
  - [x] B1e. Move the remaining server producers and wrappers (JRuby source
        navigation, extension seed file, project batch and retained facts) to
        `FileAnalysis`.
  - [x] B1f. Delete the `FileFacts` alias and update the analysis, engine, and
        collector guides and `src/ARCHITECTURE.md`.

  Notes: `replace_facts` keeps its name until B4 renames it to
  `Project::update`. `FileAnalysis` has no `file_id` field until B4. Flow
  types, extension patches, and the document stay out of `FileAnalysis`;
  they move when C1 makes the loader own composition. The persisted gem
  product DTO does not change, so its schema stays the same. Skipping the
  didOpen re-analysis (which reruns the collector to rebuild local variable
  scopes) is a separate follow-up after C1: either a bounded scope cache keyed
  by source snapshot, or a scope-only walk.
- [x] B2. Add `FileOwned<T>` for rows that belong to a file. Port the existing
      stores to it, one store per commit.

  Notes: `core/storage/file_owned` holds three shapes. `FileOwned<T>` keeps
  each file's rows in one sorted vector (diagnostics, diagnostic candidates,
  reference candidates). `FileArena<T>` gives rows stable ids for stores
  with cross-file lookups, and `FileIndex<K>` keeps each key's ids grouped
  by file so replacement cuts and splices one run (symbols, methods). Not
  ported:
  - `TypeStore` is also the collector's append-only working store. `add`
    appends one fact at a time in source order, and the in-place equation
    updates rewrite facts by subject. Moving it needs the collector to own
    a separate append buffer, which belongs with D2.
  - `SemanticGraph` merges node definitions from many files under one FQN,
    and graph resolution adds edges and re-queues unresolved edges one at a
    time after the file's replacement. Its adjacency lists are per node,
    not per file. B3's `Hierarchy` should redesign it rather than wrap it.
  - `ReferenceStore` is keyed by target, not file. Each workspace pass
    rebuilds it, and `facts_for` returns borrowed slices on the
    references hot path.
- [ ] B3. Extract the `AnalysisEngine` components one at a time: `Files`,
      `Names`, `DeclIndex`, `Hierarchy`, `UseIndex`, `TypeTable`, `Solver`,
      `Diagnostics`. Each owns its impls in its own module.
  - [x] B3a. Move `engine/state/fingerprint/` and
        `engine/state/external_facts_template/` to a new `engine/persist/` so
        `engine/state/` has room for one module per component. No behavior
        change.
  - [x] B3b. Extract `Names` (`engine/state/names.rs`): `NameRegistry`, its
        test hooks, `fqn_for_id`, and `expand_interned_fqn`. Interned ids stay
        `pub(in crate::engine)`; components that intern take `&mut Names`.
  - [x] B3c. Extract `Files` (`engine/state/files.rs`): source registry, file
        id map, source files and line indexes, snapshot issuing, registration,
        and the export fingerprint map. Delete `state/file_id_map.rs`.
  - [x] B3d. Extract `Diagnostics` (`engine/diagnostics/store.rs`): candidate
        and resolved stores, candidate install, the unresolved-require swap,
        and the resolved rebuild filter used by the workspace pass.
  - [x] B3e. Extract `UseIndex` (`engine/state/uses.rs`): reference candidate
        and resolved stores, candidate interning, reference reads, and
        take/restore of candidates for the workspace pass.
  - [x] B3f. Extract `DeclIndex` (rename `state/facts.rs` to `decls.rs`):
        symbols, methods, visibility overrides, execution contexts, their
        interning and expansion, and the effective-method reads.
  - [x] B3g. Extract `Hierarchy` (rename `state/graph.rs` to `hierarchy.rs`):
        graph nodes and edges, constant path resolution, unresolved-edge
        retry, and the method-lookup-chain caches with their invalidation.
        Route direct `engine.graph`/`engine.names` reads through component
        methods.
  - [x] B3h. Extract `TypeTable` (`engine/state/types.rs`): `TypeStore`, call
        expression outcomes, local-read types, the outcome merge, and the
        target writers the solver uses. Delete `state/storage.rs`; memory
        stats and `shrink_to_fit` delegate per component. Profiler comparison.
  - [ ] B3i. Extract `Solver` (rename `state/inference.rs` to `solver.rs`):
        inference evidence, dirty flags, equation solving split into a
        read-only plan step and an `apply` step that writes `TypeTable`.
        Profiler comparison.
  - [ ] B3j. Leave `state/lifecycle.rs` as orchestration over the components,
        with `semantic_revision` and `query_cache_identity` on the engine.
        Update `engine/README.md` and the analysis README.
  - [ ] B3k. Add the read-only `Semantics` trait (`engine/semantics.rs`) for
        mid-walk reads and switch the fact collector and `TypeTracker` to it
        instead of `Arc<RwLock<AnalysisEngine>>`.

  Notes: keep the single engine `RwLock`; lock granularity is C4. Every B3
  commit changes the gem producer fingerprint in `build.rs`, which forces one
  cold gem reindex; that is expected. Semantic fingerprints must hash the same
  data in the same order (`state/tests/fingerprints.rs`).
- [ ] B4. Reduce `AnalysisEngine` to `Project` with `update`, `remove`,
      `resolve`, and `view`.
- [ ] B5. Add `lookup::method(view, MethodRequest) -> MethodAnswer` and replace
      the method-lookup variants with it.
- [ ] B6. Move `AnalysisQuery` methods to free functions over `View`, one
      feature at a time: definition, references, hover, completion, inlay
      hints, diagnostics.
- [ ] B7. Gather diagnostic policy into one module.
- [ ] B8. Break the indexer ↔ inference ↔ engine cycle so dependencies point
      one way.

## Phase C: server (`src/`)

- [ ] C1. Rename `src/indexer` to `loader` and make it a function of a
      `LoadContext` that returns `FileAnalysis` values.
  - [x] C1a. Rename `src/indexer` to `src/loader` with no other change. Rewrite
        `crate::indexer` and `ruby_fast_lsp::indexer` paths (not
        `ruby_analysis::indexer`), `build.rs` `GEM_FACT_PRODUCER_TREES`,
        `crates/devtools`, `src/main.rs`, and the guides that name the folder.
        The gem producer fingerprint changes, so expect one cold gem reindex.
  - [x] C1b. Move `lsp::capabilities::diagnostics::generate_diagnostics`
        (syntax diagnostics) to `src/loader/syntax_diagnostics.rs`. This
        removes the two production loader → lsp edges.
  - [ ] C1c. Add `src/loader/context.rs` with `LoadContext`, `LoadConfig`,
        `RequireContext`, `SharedProducts`, `SourceReader`, and
        `RuntimeDiscovery`. Build it in `init_workspace_inner` and in the
        interactive path from the server, and pass it next to `server`. No
        reads move yet.
  - [ ] C1d. Switch reads from `server` to `ctx`, one area per commit: config
        and require roots (delete `RequireDiagnosticRoots::Server`), products
        and the governor, runtime discovery, open buffers through
        `SourceReader`.
  - [ ] C1e. Add the `LoadSink` trait and a server adapter. Route source
        registration, commits, resolve, runtime setters, status, and progress
        through it, one source (`stdlib`, `gems`, `project`, `jruby`) per
        commit. `src/server` is at 10 entries, so the adapter replaces a file
        or lives under `projects`. Profiler comparison.
  - [ ] C1f. Make `analyze_file` return `LoadedFile`: the caller commits and
        updates `documents`. Only the seed commit stays inside, through
        `LoadSink::commit_seed`, until B3k. Profiler comparison, with didOpen
        p95 called out.
  - [ ] C1g. Remove the `server` parameter from the loader. Publish
        open-project diagnostics from `LoadSink::project_facts_ready` on the
        server side. Update `src/loader/README.md`, `src/ARCHITECTURE.md`, and
        `docs/development/server-state.md`.

  Notes: the loader must never take `&RubyLanguageServer` after C1g. The
  seed-before-walk commit goes away when `analyze_file` takes `&dyn
  Semantics` (B3k/B4). `LoadContext` is a cloneable value of `Arc` fields that
  replaces the loader's reads of the server; `LoadSink` replaces its writes.
- [ ] C2. Break the indexer ↔ server, lsp ↔ server, and indexer ↔ environment
      cycles.
  - [ ] C2a. Move `ProjectRuntimeStatus` and its use of
        `ProjectIndexingSnapshot` from `environment/runtime/catalog.rs` to
        `server/products.rs`.
  - [ ] C2b. Move `loader/scheduling/resources` to `src/utils/admission/` with
        no other change.
  - [ ] C2c. Make the persistent cache generic over a `PersistentProduct`
        trait (kind, key, encode, decode). Keep the namespace and magic
        constants so existing caches stay valid. `GemDependencyProduct` and
        `JavaArtifactProduct` implement it in their own modules.
  - [ ] C2d. Move `loader/cache/persistent` to `src/utils/persistent_cache/`
        with no other change. Environment then no longer imports the loader.
  - [ ] C2e. Move `collect_project_files` from `utils/file_ops.rs` to
        `loader/sources/project`, so utils no longer reads `IndexingConfig`.
  - [ ] C2f. Make the extension registry return the seed `FileAnalysis`
        instead of writing the engine. The loader commits it through
        `LoadSink::commit_seed`.
  - [ ] C2g. Move `impl LanguageServer` and the debug and namespace-tree
        request methods from `server/mod.rs` to `src/lsp/service.rs`. The
        server keeps state only.
  - [ ] C2h. Move the namespace-tree response types to
        `server/namespace_tree.rs` and the engine diagnostic projection to
        `server/diagnostics.rs`. Move
        `refresh_unresolved_require_diagnostics_for_workspace` up to lsp
        lifecycle if it still needs the linter.
  - [ ] C2i. Add a layering check to `support/structure/check.py`: no
        `crate::server`, `crate::lsp`, or `crate::features` in `src/loader`,
        `src/environment`, or `src/utils`, and no `crate::lsp` in
        `src/server`. Move loader and environment tests that drive lsp
        handlers to `src/test/integration/`.
- [ ] C3. Merge the handler, capability, and query layers into one `features/`
      module per feature.
  - [ ] C3a. Add `src/features/` with `mod.rs` and a README. Move `lsp/query`
        shared code (`EngineQuery`, `analysis_location`, `method`) to
        `features/cursor/` with no other change.
  - [ ] C3b. Navigation: merge capability, query, and handler body for
        definition, implementation, references, highlights, hierarchies,
        workspace symbols, and namespace tree into `features/navigation/`.
        One commit per two or three features.
  - [ ] C3c. Presentation: hover, inlay hints, code lens, document symbols,
        folding, selection ranges, and semantic tokens into
        `features/presentation/`.
  - [ ] C3d. Editing: completion, signature help, formatting, rename, and code
        actions into `features/editing/`.
  - [ ] C3e. Diagnostics and debug: engine projection and linter into
        `features/diagnostics/`, and debug and extension status into
        `features/debug.rs`.
  - [ ] C3f. Move `capabilities/indexing` and `handlers/notification` to
        `src/lsp/lifecycle/`. Delete `lsp/capabilities`, `lsp/query`, and
        `lsp/handlers`. Update devtools, the test harness, and the guides.
  - [ ] C3g. Extend the layering check: features may use `server` and
        `loader` but not `lsp`, and `lsp` reaches features only through
        `handle`. Update `src/ARCHITECTURE.md` and the skills.

  Notes: each feature exposes `handle(server, params)`, absorbing its body
  from `handlers/request.rs`. Split any merged file over 1,000 lines inside
  its feature folder.
- [ ] C4. Introduce `ProjectHandle`. A single writer task owns each project's
      mutations and readers share the project for reads. Remove the per-project
      locks from `RubyLanguageServer`.
  - [ ] C4a. Add `ProjectHandle` wrapping the existing
        `Arc<RwLock<AnalysisEngine>>` with `view()` and `update(|engine| ..)`.
        No behavior change.
  - [ ] C4b. Route feature reads through `view()` with one guard per request.
        Remove the repeated `.read()` calls in references, hover, and
        definition.
  - [ ] C4c. Add the writer task: a command channel (commit-if-snapshot,
        register, resolve chunk, reset) with replies through oneshot. The
        `LoadSink` adapter sends commands. Profiler comparison, with didOpen
        and cold indexing called out.
  - [ ] C4d. Move the lifecycle writes (clear facts, runtime-rebuild reset,
        the require-diagnostic refresh, the extension seed, and project
        engine setup) to commands.
  - [ ] C4e. Fold `ProjectRuntimeState`'s four locks and the require index
        into the handle's state. Remove `Workspace::analysis_engine` and
        `analysis_engine_for_uri`. Update `docs/development/server-state.md`.
- [ ] C5. Reduce the server to `Server { client, config, documents, projects }`.
- [ ] C6. Put JRuby support behind the existing `jruby-support` crate boundary
      so the server only sees an add-on interface.
- [ ] C7. Use one Ruby version detector and one RSpec implementation.

## Phase D: one walk, one flow engine

- [ ] D1. Merge the three declaration walkers into one `Walk`.
- [ ] D2. Merge collector flow inference and `TypeTracker` into one `Flow`.
      Compare the profiler output before and after.
- [ ] D3. Rewrite `src/ARCHITECTURE.md`, the analysis README, and `AGENTS.md`
      ownership tables to match the final layout. Remove this plan.

## Open questions

Settle these before the task that needs them.

- B3/D2 (settled): No. Constant types, dispatched method-return
  dependencies, and chained receivers are already equations and stay so. Two
  kinds of mid-walk reads cannot become equations without a new flow solver:
  reads that decide which facts get emitted (namespace or singleton receiver,
  `initialize` inside a class) and reads that feed local flow (RBS parameter
  and return contracts, higher-order block parameters, callable constant
  bodies, method returns through dispatch, `super`, and constructors). Options
  were (A) turn everything into equations, which is the D2 rewrite and doubles
  candidate facts for emission-shaping reads; (B) a read-only `Semantics` trait
  for every mid-walk read; (C) a hybrid. Chosen: C. The walk reads through a
  read-only `Semantics` trait (B3k), which `View` implements after B4 and which
  also serves B8. Any new mid-walk read must become an equation or be added to
  `Semantics` with a reason. D2 may move flow-feeding reads into equations one
  at a time, each with a profiler comparison.
- C4 (settled): Readers take one read lock per request through
  `ProjectHandle::view()`, not an immutable snapshot. Options were (A) one
  read lock per request, (B) an immutable snapshot per commit (whole-engine
  clone or persistent maps), (C) A plus `Arc` snapshots of cheap derived
  products. Cloning the engine per commit costs up to the 32 MiB heap budget
  on every edit, breaks `SourceFileSnapshot` identity because a clone gets a
  new `instance_id`, and does not fit the 100 ms body-edit p95. Persistent
  data structures would rewrite every B3 component. Chosen: C. A single writer
  task per project serializes `update`, `remove`, and `resolve`, checks
  source snapshots, keeps open buffers authoritative, and commits batches and
  workspace resolve in bounded chunks so no reader waits for a whole batch. A
  request holds one `View` for its whole run, so its answer reflects one
  semantic revision. Cheap derived products (namespace tree, symbol lists)
  are published as `Arc` values keyed by `semantic_revision`. Revisit
  per-component persistent storage after B4 if the profiler shows reader
  stalls.
