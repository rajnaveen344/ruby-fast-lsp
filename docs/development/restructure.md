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
- [ ] B2. Add `FileOwned<T>` for rows that belong to a file. Port the existing
      stores to it, one store per commit.
- [ ] B3. Extract the `AnalysisEngine` components one at a time: `Files`,
      `Names`, `DeclIndex`, `Hierarchy`, `UseIndex`, `TypeTable`, `Solver`,
      `Diagnostics`. Each owns its impls in its own module.
  - [ ] B3a. Move `engine/state/fingerprint/` and
        `engine/state/external_facts_template/` to a new `engine/persist/` so
        `engine/state/` has room for one module per component. No behavior
        change.
  - [ ] B3b. Extract `Names` (`engine/state/names.rs`): `NameRegistry`, its
        test hooks, `fqn_for_id`, and `expand_interned_fqn`. Interned ids stay
        `pub(in crate::engine)`; components that intern take `&mut Names`.
  - [ ] B3c. Extract `Files` (`engine/state/files.rs`): source registry, file
        id map, source files and line indexes, snapshot issuing, registration,
        and the export fingerprint map. Delete `state/file_id_map.rs`.
  - [ ] B3d. Extract `Diagnostics` (`engine/diagnostics/store.rs`): candidate
        and resolved stores, candidate install, the unresolved-require swap,
        and the resolved rebuild filter used by the workspace pass.
  - [ ] B3e. Extract `UseIndex` (`engine/state/uses.rs`): reference candidate
        and resolved stores, candidate interning, reference reads, and
        take/restore of candidates for the workspace pass.
  - [ ] B3f. Extract `DeclIndex` (rename `state/facts.rs` to `decls.rs`):
        symbols, methods, visibility overrides, execution contexts, their
        interning and expansion, and the effective-method reads.
  - [ ] B3g. Extract `Hierarchy` (rename `state/graph.rs` to `hierarchy.rs`):
        graph nodes and edges, constant path resolution, unresolved-edge
        retry, and the method-lookup-chain caches with their invalidation.
        Route direct `engine.graph`/`engine.names` reads through component
        methods.
  - [ ] B3h. Extract `TypeTable` (`engine/state/types.rs`): `TypeStore`, call
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
- [ ] C2. Break the indexer ↔ server, lsp ↔ server, and indexer ↔ environment
      cycles.
- [ ] C3. Merge the handler, capability, and query layers into one `features/`
      module per feature.
- [ ] C4. Introduce `ProjectHandle`. A single writer task owns each project's
      mutations and readers share the project for reads. Remove the per-project
      locks from `RubyLanguageServer`.
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
- C4: Should readers take a read lock or an immutable snapshot?
