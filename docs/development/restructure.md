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
- [ ] A7. Replace scattered statistics plumbing with one stats registry.
- [x] A8. Remove `mod.rs` files that only re-export.

## Phase B: analysis data model (`crates/ruby-analysis`)

- [ ] B1. Introduce `FileAnalysis` as the single per-file output. Keep it
      alongside `FileFacts`, `AnalysisIndex`, and `CollectedFile`, then remove
      those three.
  - [ ] B1a. Move `engine::FileFacts` to `core::FileAnalysis`
        (`core/source/file_analysis.rs`) with the same fields, and leave
        `FileFacts` as a temporary alias. Switch the engine internals
        (lifecycle, fingerprint, template, codec).
  - [ ] B1b. Make `AnalysisIndexer` and `index_rbs` return `FileAnalysis` and
        type the collector's direct facts as `FileAnalysis`. Delete
        `AnalysisIndex`, `file_analysis_facts_from_index`, and the RBS and
        stdlib field-copy blocks.
  - [ ] B1c. Give the collector one `FileAnalysis`. `finish()` returns
        `FactCollectorOutput { analysis, flow_types, extension_patches,
        document }`. Delete `CollectedFile`. Add
        `FileAnalysis::replace_declarations`.
  - [ ] B1d. Merge the two server assembly paths (`process_file` and the
        batch collection path) into one `compose_file_analysis`, with a
        profiler run before and after.
  - [ ] B1e. Move the remaining server producers and wrappers (JRuby source
        navigation, extension seed file, project batch and retained facts) to
        `FileAnalysis`.
  - [ ] B1f. Delete the `FileFacts` alias and update the analysis, engine, and
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

- B3/D2: Can every engine query made in the middle of a walk become an
  equation? If not, use a read-only `Semantics` trait as the fallback.
- C4: Should readers take a read lock or an immutable snapshot?
