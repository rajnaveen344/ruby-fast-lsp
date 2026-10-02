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
- [x] B3. Extract the `AnalysisEngine` components one at a time: `Files`,
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
  - [x] B3i. Extract `Solver` (rename `state/inference.rs` to `solver.rs`):
        inference evidence, dirty flags, equation solving split into a
        read-only plan step and an `apply` step that writes `TypeTable`.
        Profiler comparison.
  - [x] B3j. Leave `state/lifecycle.rs` as orchestration over the components,
        with `semantic_revision` and `query_cache_identity` on the engine.
        Update `engine/README.md` and the analysis README.
  - [x] B3k. Add the read-only `Semantics` trait (`engine/semantics.rs`) for
        mid-walk reads and switch the fact collector and `TypeTracker` to it
        instead of `Arc<RwLock<AnalysisEngine>>`.

  Notes: keep the single engine `RwLock`; lock granularity is C4. Every B3
  commit changes the gem producer fingerprint in `build.rs`, which forces one
  cold gem reindex; that is expected. Semantic fingerprints must hash the same
  data in the same order (`state/tests/fingerprints.rs`).
- [ ] B4. Reduce `AnalysisEngine` to `Project` with `update`, `remove`,
      `resolve`, and `view`.
  - [x] B4a. Rename `AnalysisEngine` to `Project` and `AnalysisQuery` to
        `View`; keep `pub type` aliases in `engine/mod.rs` until C1 lands.
  - [x] B4b. Rename `replace_facts` to `update` and
        `replace_facts_if_source_snapshot` to `update_if_snapshot`; migrate
        callers.
  - [x] B4c. Add `Project::view()`; migrate `query()` callers. `query()`
        forwards to `view()` until the loader callers migrate (B4h).
  - [x] B4d. `impl Semantics for View`; the `RwLock<Project>` impl takes a
        guard and delegates.
  - [ ] B4e. Move read-only methods from `Project` to `View`, one component
        per commit (files, decls, hierarchy, diagnostics, solver telemetry,
        fingerprints).
  - [x] B4f. Add `Project::remove(file_id)` and `remove_if_snapshot`: drop
        the file from every component, `Files` maps, and export fingerprints;
        advance the revision; re-queue dependents. Ids are never reused. Tests
        in `engine/state/tests/remove.rs`: removal equals never-added
        fingerprints, stale snapshots rejected, re-register does not resurrect
        facts, edges into the removed file become unresolved, unknown id is a
        no-op.
  - [x] B4g. Replace clear-by-empty-facts with `remove` in the server
        (`clear_file_facts_if_kind`, project collection, semantic context),
        routed through `LoadSink` (after C1e). Server test: a deleted watched
        file drops its diagnostics and references. Done for deletion,
        policy exclusion, closed excluded documents, and rehomed documents.
        The project collection and semantic context sites stay empty updates:
        they withhold facts on a private snapshot whose seed addresses the
        same files by path, and no file leaves the project there. Unreadable
        or unparsable files that still exist also stay registered and empty.
  - [ ] B4h. Remove the aliases; update the engine and crate READMEs.
- [x] B5. Add `lookup::method(view, MethodRequest) -> MethodAnswer` and replace
      the method-lookup variants with it.
  - [x] B5a. Add `engine/lookup/` with `MethodRequest { receiver, method,
        access, want }` and `MethodAnswer { Found, Ambiguous, Missing,
        Unknown }`, delegating to the existing inner functions. Equality tests
        against the legacy functions per want × access × receiver.
  - [x] B5b. Fold `MethodLookupResult` and `EffectiveMethodFactMatch` into
        `MethodAnswer`.
  - [x] B5c. Make the public/protected/`_for_type`/`_cached` wrappers
        one-liners over `lookup::method`; delete those with no callers.
  - [x] B5d. Make the return-type walk consume `MethodAnswer` instead of
        `method_facts_in_chain`. Profiler comparison.
  - [x] B5e. Replace the three method memo maps with one keyed on
        `MethodRequest`. Profiler comparison.
  - [x] B5f. Migrate server callers; delete the remaining legacy wrappers.
  - [x] B5g. Make `Semantics` method reads call `lookup::method`. Profiler
        comparison.

  Notes: B5f turned the B5a equality tests in `engine/lookup/tests.rs` into
  an expected-answer table (`lookup/expected_answers.txt`, one line per
  receiver, method, and want over the four accesses; regenerate with
  `LOOKUP_EXPECTED_BLESS=1` and review the diff). After B5g no
  access-flavoured `View` wrapper remains: `Semantics` states its request
  and the view's walk memo serves it, so `method_cached` is gone too
  (`View::with_memo` attaches a cache). B5d decision on the builtin constructor
  check in `call_outcomes.rs`, which treats `Unknown` like `Missing`: keep
  it for now. The fallback also requires the instance namespace to be an
  indexed class, so `Unknown(Receiver)` never yields a type there. With
  `Unknown` mapped to "no fallback", the full suite still passes, and
  `Foo.new` for `class Foo < UnindexedBase` keeps its `Foo` return
  (pinned in `method_chaining.rs`); no fixture reached the arm with an
  incomplete chain. Options: (a) keep;
  (b) map `Unknown` to no fallback, the strict reading of "never replace
  uncertainty with a guessed type", since an unindexed ancestor may define
  `self.new`; (c) keep the fallback but record that the type is a language
  default rather than a proven `Class#new`. Recommendation: (b) as its own
  bug-fix commit once a fixture shows the arm producing a type, because
  today no observable output distinguishes (a) from (b).
  `MethodAnswer::Unknown` carries the rule that unknown lookup edges
  suppress missing-method claims (file pass, grouped methods, workspace pass,
  rename). Answers are derived data and never enter fingerprints.
- [ ] B6. Move `AnalysisQuery` methods to free functions over `View`, one
      feature at a time: definition, references, hover, completion, inlay
      hints, diagnostics.
  - [ ] B6a. `EngineQuery::with_view` takes exactly one read guard per
        request.
  - [ ] B6b. Definition, implementation, and the shared method module.
  - [ ] B6c. References and document highlights.
  - [ ] B6d. Hover.
  - [ ] B6e. Completion. Profiler comparison after B6b–e, including writer
        wait time.
  - [ ] B6f. Inlay hints, signature help, rename, hierarchies, code lens,
        workspace symbols.
  - [ ] B6g. Diagnostics projection takes `&View` (with B7d).
  - [ ] B6h. Only `Project::view()` constructs views outside `engine/`.

  Notes: B6 changes signatures and C3 moves files; never mix them in one
  commit. Do B6 for a feature before its C3 move, or after it lands.
- [ ] B7. Gather diagnostic policy into one module.
  - [ ] B7a. `engine/diagnostics/policy.rs` absorbs `helpers.rs` and owns the
        code constants and severities.
  - [ ] B7b. Move the suppression predicates (incomplete chain, explicit
        contract, dynamic mixin hook) into `policy.rs`; rename uses the same
        predicate, with a test for fail-closed rename on an incomplete chain.
  - [ ] B7c. The server imports the engine's `unresolved-require` code.
  - [ ] B7d. One engine-to-LSP projection; delete the coordinator's fast copy
        (keep the faster implementation). Measure open-project publish.
  - [ ] B7e. One composition function for syntax, engine, and linter
        diagnostics; replace the hand-assembled publish sites.
  - [ ] B7f. Move composition with C3e; the linter stays a runner.
- [x] B8. Break the indexer ↔ inference ↔ engine cycle so dependencies point
      one way: core ← inference ← indexer ← engine.
  - [x] B8a. Move `MethodReceiver` and `VariableTypeKind` to `core`.
  - [x] B8b. Move `inference/completion/` to `engine/queries/completion/`.
  - [x] B8c. Move callable-literal lowering helpers into inference.
  - [x] B8d. Move `Semantics`, `ReceiverAccess`, and `LocalType` to
        `inference/semantics.rs`; the engine keeps the impls.
  - [x] B8e. Inference takes `&dyn Semantics` instead of `AnalysisQuery`.
        Profiler comparison; use generics if dispatch costs show.
  - [x] B8f. The fact collector and receiver queries take `dyn Semantics`.
        Profiler comparison.
  - [x] B8g. Architecture test: no upward `crate::` edges in non-test files.

  Notes: inference and the fact collector also take `AnalysisQueryCache`
  (an engine type) directly; B8e–f must hide it behind `Semantics` (the
  `View` carries its cache) rather than move the memo down a layer.
  `inference/` and `engine/diagnostics/` are full; B8b frees the slot
  B8d needs. B4g, B7d–e, and B8f touch the loader, so they follow C1e; C1f
  follows B8f.

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
  - [x] C1c. Add `src/loader/context.rs` with `LoadContext`, `LoadConfig`,
        `RequireContext`, `SharedProducts`, `SourceReader`, and
        `RuntimeDiscovery`. Build it in `init_workspace_inner` and in the
        interactive path from the server, and pass it next to `server`. No
        reads move yet.
  - [x] C1d. Switch reads from `server` to `ctx`, one area per commit: config
        and require roots (delete `RequireDiagnosticRoots::Server`), products
        and the governor, runtime discovery, open buffers through
        `SourceReader`.
  - [x] C1e. Add the `LoadSink` trait and a server adapter. Route source
        registration, commits, resolve, runtime setters, status, and progress
        through it, one source (`stdlib`, `gems`, `project`, `jruby`) per
        commit. `src/server` is at 10 entries, so the adapter replaces a file
        or lives under `projects`. Profiler comparison.
  - [ ] C1f. Make `analyze_file` return `LoadedFile`: the caller commits and
        updates `documents`. Only the seed commit stays inside, through
        `LoadSink::commit_seed`, until B3k. Profiler comparison, with didOpen
        p95 called out.
  - [x] C1g. Remove the `server` parameter from the loader. Publish
        open-project diagnostics from `LoadSink::project_facts_ready` on the
        server side. Update `src/loader/README.md`, `src/ARCHITECTURE.md`, and
        `docs/development/server-state.md`.

  Notes: the loader must never take `&RubyLanguageServer` after C1g. The
  seed-before-walk commit goes away when `analyze_file` takes `&dyn
  Semantics` (B3k/B4). `LoadContext` is a cloneable value of `Arc` fields that
  replaces the loader's reads of the server; `LoadSink` replaces its writes.
- [ ] C2. Break the indexer ↔ server, lsp ↔ server, and indexer ↔ environment
      cycles.
  - [x] C2a. Move `ProjectRuntimeStatus` and its use of
        `ProjectIndexingSnapshot` from `environment/runtime/catalog.rs` to
        `server/products.rs`.
  - [x] C2b. Move `loader/scheduling/resources` to `src/utils/admission/` with
        no other change. The inner `admission.rs` keeps its name.
  - [x] C2c. Make the persistent cache generic over a `PersistentProduct`
        trait (kind, key, encode, decode). Keep the namespace and magic
        constants so existing caches stay valid. `GemDependencyProduct` and
        `JavaArtifactProduct` implement it in their own modules. Done: the
        kind stays a closed `PersistentProductKind` enum because namespace
        scans, eviction, and per-kind counters enumerate it; compiled Wasm
        keeps its byte-artifact API. Gem and Java cache tests moved next to
        their products, with schema-1 compatibility tests that write the
        envelope from literal constants.
  - [x] C2d. Move `loader/cache/persistent` to `src/utils/persistent_cache/`
        with no other change. Environment then no longer imports the loader.
        Done: environment has no `crate::loader` import, tests included.
  - [x] C2e. Move `collect_project_files` from `utils/file_ops.rs` to
        `loader/sources/project`, so utils no longer reads `IndexingConfig`.
        Done: `ProjectFilePolicy`, `collect_project_signature_files`, and
        their glob helpers also read `IndexingConfig`, so they moved with it
        to `loader/sources/project/files.rs`; `should_index_file` and the
        configuration-free Ruby collectors stay in utils.
  - [x] C2f. Make the extension registry return the seed `FileAnalysis`
        instead of writing the engine. The loader commits it through
        `LoadSink::commit_seed`. Done in one piece: the registry produces an
        `ExtensionSemanticSeed` (registry `seed.rs`) whose `source()` and
        `analysis(file_id)` the loader registers and commits. The seed
        ledger (engine identity and applicability fingerprint) stays in the
        registry, and the registry hands the seed to a caller-supplied
        commit inside the ledger lock, so ordering is unchanged and nothing
        is committed late. `analyze_file` and the project baseline seed
        commit through `LoadSink::commit_seed`. The shared collection core
        also serves sink-less entry points on caller-supplied engines
        (`*_in_engine`, the project batch worker, devtools), so it calls the
        same loader write, `commit_extension_seed`, directly; it moves to
        the sink when collection takes a sink or when B3k removes the
        seed-before-walk commit. Environment has no production engine write;
        JRuby navigation tests still build local fixture engines.
  - [x] C2g. Move `impl LanguageServer` and the debug and namespace-tree
        request methods from `server/mod.rs` to `src/lsp/service.rs`. The
        server keeps state only.
  - [x] C2h. Move the namespace-tree response types to
        `server/namespace_tree.rs` and the engine diagnostic projection to
        `server/diagnostics.rs`. Move
        `refresh_unresolved_require_diagnostics_for_workspace` up to lsp
        lifecycle if it still needs the linter. Done: the cached response is
        the engine's `NamespaceTreeResponse`, so the server imports it from
        `ruby_analysis` and `NamespaceTreeParams` stays with the lsp feature;
        the refresh reads only server-retained linter output, so it stays.
  - [x] C2i. Add a layering check to `support/structure/check.py`: no
        `crate::server`, `crate::lsp`, or `crate::features` in `src/loader`,
        `src/environment`, or `src/utils`, and no `crate::lsp` in
        `src/server`. Move loader and environment tests that drive lsp
        handlers to `src/test/integration/`. Done: the three server-driven
        extension tests moved to `integration/lifecycle/extension_workspaces.rs`,
        so environment is clean, tests included. Ten loader test modules
        build loads through the server's `LoadSink` or assert through lsp
        adapters while reaching private loader internals through `super`;
        they are named exemptions in `policy.json`, and the check fails once
        an exempt file stops violating, so the list can only shrink.
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
- [x] C6. Put JRuby support behind the existing `jruby-support` crate boundary
      so the server only sees an add-on interface.
  - [x] C6a. Bug: the persisted gem product identity hashed the JRuby import
        producers but not the workspace crates they and the fact collector
        call (`jruby-support` proxy names, `jvm-metadata` class parsing, and
        the `extension-api` contract), so a change there could reuse stale
        gem facts. `build.rs` lists those trees, and
        `loader/cache/producer_identity_tests.rs` fails when a listed
        producer calls a workspace crate whose tree is not listed. Expect one
        cold gem reindex.
  - [x] C6b. One add-on value. The server holds `Option<JrubyAddOn>` per
        project instead of an import provider and a separate classpath
        fingerprint lock, and the loader hands it over through one
        `LoadSink` write:

        ```rust
        // src/loader/jruby_add_on.rs
        #[derive(Clone, Debug)]
        pub struct JrubyAddOn { /* Arc<JrubyImportProvider> */ }
        impl JrubyAddOn {
            pub(in crate::loader) fn new(imports: Arc<JrubyImportProvider>) -> Self;
            pub fn classpath_fingerprint(&self) -> &str;
            pub(in crate::loader) fn import_provider(&self) -> &Arc<JrubyImportProvider>;
        }
        // LoadSink (replaces clear_/install_jruby_import_provider)
        fn set_jruby_add_on(&self, root: &Path, add_on: Option<JrubyAddOn>);
        // loader entry point for interactive processors
        FileProcessor::with_jruby_add_on(self, add_on: &JrubyAddOn) -> Self;
        ```

        The server reads only the fingerprint (runtime status, profiler)
        and passes the add-on back to the loader; the provider accessor is
        visible only inside `crate::loader`, so the compiler keeps
        `server` and `lsp` off the import provider. Provider and
        fingerprint now change in one write instead of two ordered ones.
  - [x] C6c. Move the catalog-independent Java DSL syntax scans (dotted
        Java calls, canonical `Java::` paths, static `java_import` /
        `include_package` dependencies, the static import-alias block
        evaluator, the gem prefilter, and `StaticJavaSourceHint`) from
        `environment/runtime/jruby/imports/{syntax,static_scan}.rs` to
        `jruby-support`. They depend only on Prism, so the crate gains a
        `ruby-prism` dependency; it still has no LSP, filesystem, or
        `ruby-analysis` dependency. C6a already hashes the crate tree, so
        the gem producer identity keeps covering them.

  Notes: what stays in `src/environment/runtime/jruby/` and why. The import
  provider's fact-collector extension (`imports/` call host, declarations,
  Java methods and types, navigation) lowers into `ruby_analysis` facts
  through `FactCollector`, which the crate's charter excludes. Classpath
  discovery, the Java artifact catalog, runtime source materialization, and
  source navigation read the filesystem and use the server's
  `persistent_cache` and `single_flight` utilities. The decompiler runs
  child processes under the server's CPU/memory/time limits. Moving any of
  them would drag `src/` types into the crate, so they stay behind the
  add-on. The loader keeps its JRuby orchestration
  (`coordinator/jruby.rs`, catalog-sensitive replay, Java navigation
  demand) because those are load steps, not server state.
- [ ] C7. Use one Ruby version detector and one RSpec implementation.
  - [x] C7a. Take gem discovery's active engine from the selected runtime
        descriptor and delete its `RUBY_ENGINE` probe. Survivor: the runtime
        catalog, which already classified the executable from its `-v`
        output. The probe ran only when no descriptor engine was set, and gem
        discovery cannot run without a descriptor, so it was unreachable.
  - [x] C7b. Bug: a project whose own runtime selection is `auto` and has no
        installed match still picked the global legacy `rubyVersion` stubs,
        because the version step re-read the global setting instead of the
        project's effective selection. The version step now reads the legacy
        family from the selection the runtime step resolved.
  - [x] C7c. One parser and one version type. `RubyVersion` moves from
        `loader/version` to `environment/runtime/version.rs` and uses the
        catalog's `RuntimeImplementation` instead of a second
        implementation enum. `parse_ruby_family` (with `RubyVersion::parse`
        and `from_family`) replaces `Config::get_ruby_version`,
        `config::runtime::parse_family`, and the coordinator's
        `ruby_version_for_runtime`. Survivor: the `u16` parse that selection
        already used; families that do not fit a `u8` keep today's bundled
        fallback. Not merged: the catalog's
        `clean_version`/`version_family`, which parse free-form `ruby -v`
        output rather than a family string.
  - [ ] C7d. RSpec (blocked on a product decision). There are two
        implementations of the same patch contract: the native fallback
        `crates/extension-rspec`, which the extension host runs in process
        whenever no loaded Wasm package claims the call, and the
        `extensions/rspec-ruby` package. Differences:
    - The npm server package ships no extension packages, so the native
      fallback is its only RSpec support. The VSIX ships the package, which
      then takes precedence.
    - The native fallback ignores the package's applicability gate
      (`rspec-core >= 3, < 4` in the lockfile) and runs in every project.
    - Only the package provides RSpec document symbols and code lenses.
    - 15 library integration tests in `src/test/integration/sources/extensions.rs`
      exercise RSpec only through the native fallback.

    Deleting either one removes a feature or changes output, so it is not
    done here. Options:
    1. Keep the package (the plan's direction): ship the RSpec package in the
       npm package and load bundled packages by default, port the 15 tests to
       load the package, then delete `crates/extension-rspec` and its host
       special case. Projects without a locked `rspec-core` stop getting
       RSpec facts.
    2. Keep the native crate: move document symbols and code lenses into it
       and drop the Ruby package. This keeps a privileged in-process
       extension, against the plan's direction.
    3. Keep both and add a parity test that runs each RSpec integration case
       through both, so they cannot drift.

    Recommendation: option 1, preceded by option 3's parity test so the
    switch is proven case by case.

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
