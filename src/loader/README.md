# Loader

The loader layer discovers Ruby files, parses them, and feeds analysis facts
into `ruby-analysis::engine`.

Storage is not owned here. The engine owns symbols, methods, types, graph facts,
reference candidates, resolved references, and diagnostics.

## Main Pieces

- `coordinator/`: workspace indexing orchestration, scheduling priority,
  runtime selection, JRuby companions, and resource admission
- `cache/`: gem dependency products and their semantic-input identity;
  `GemDependencyProduct` implements `PersistentProduct`, the codec the
  persistent derived-product cache in `src/utils/persistent_cache/` stores it
  through
- `context.rs`: `LoadContext`, the owner-supplied inputs the loader reads
  (live configuration, published require roots, shared products, the
  resource governor, runtime discovery, and open buffers through
  `SourceReader`), and `LoadSink`, the owner operations it performs
- `file_processor/`: parse one file and run `FactCollector`, merge collected
  facts, and convert extension-produced facts; `syntax_diagnostics.rs` there
  produces parser syntax, unreachable-code, and inconsistent-return
  diagnostics for one parsed file
- `require_paths/`: require-path resolution
- `scheduling/`: the project indexing queue, progress status, and
  navigation demand; the resource governor it admits work through lives in
  `src/utils/admission/`
- `sources/project/`: project root discovery, project file discovery
  (`files.rs` owns `ProjectFilePolicy` and the configured project and
  signature file collectors), navigation-demand collection, and dependency
  scan
- `sources/stdlib/`: standard library file discovery and exact runtime load paths
- `sources/gems/`: gem discovery, lockfile selection, vendor cache extraction,
  and shared gem dependency products

See [namespace indexing](../../docs/development/namespace-indexing.md) for how
constant-path module and class definitions map to namespaces.

## Contract

The loader is a function of a `LoadContext` that writes through a `LoadSink`.
It never sees the server: no loader entry point takes `RubyLanguageServer`,
and non-test loader code has no `crate::server` import.

## Inputs

The server builds a `LoadContext` for each whole-project load
(`load_context_for_project`) and each interactive file pass
(`load_context_for_uri`), and passes it to
`IndexingCoordinator::run_complete_indexing` and the `FileProcessor::process_file*`
entry points. Its handles read live: configuration, published require roots,
and open buffers are observed when the loader consults them. The loader reads
configuration, require roots, shared products, the resource governor, runtime
discovery, and open buffers only through this context.

## Writes and owner lookups

`LoadContext::sink` is a `LoadSink`, implemented by the server in
`src/server/projects/load_sink.rs`. Every write and owner lookup the loader
makes goes through it as one domain operation: engine and project routing,
indexing run checks and phase transitions, progress, runtime, Ruby version,
and JRuby provider selection, source registration, processed-document marks,
require-root publication and require-diagnostic refresh, navigation demand
queues, inlay-hint refresh, and extension registry and context. The loader
calls them in its own order, so the owner observes the load's write sequence.
Fact commits and resolution still run on the engine handle returned by
`engine_for_uri`.

The loader never removes a file. A file that exists but does not parse stays
registered with no facts. The pre-collection baseline withholds stale project
facts with empty updates on its private snapshot, because the declaration seed
addresses those files by path. Deleted, excluded, and rehomed files are removed
by the server with `Project::remove`.

After final resolution the loader calls `LoadSink::project_facts_ready`. The
server then publishes a complete diagnostic projection (syntax, engine facts,
and retained external linter results) for each open document that the
project's engine owns, in URI order, under the document semantic lock. It
stops once the indexing run is no longer current and returns that state; the
loader turns it into the run's cancellation error. The loader composes no LSP
diagnostics for publication.

## Current Flow

1. Scan project dependencies.
2. Collect facts from gems.
3. Collect facts from stdlib.
4. Collect facts from project files.
5. Resolve, then hand the project to the owner through `project_facts_ready`.

`FactCollector` emits reference candidates during the same pass as definitions.
The engine resolves candidates after each file update.
