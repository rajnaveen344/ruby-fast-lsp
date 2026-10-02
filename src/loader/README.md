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
- `context/`: `LoadContext`, the owner-supplied inputs the loader reads
  (live configuration, published require roots, shared products, the
  resource governor, runtime discovery, and open buffers through
  `SourceReader`), `LoadSink`, the owner operations it performs, and
  `LoadTarget` (`context/target.rs`), the named engine writes of one project
- `jruby_add_on.rs`: `JrubyAddOn`, the per-project JRuby add-on the owner
  holds. The owner sees only its classpath fingerprint and hands it back
  through `FileProcessor::with_jruby_add_on`; the import provider behind it is
  visible only inside the loader
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
and loader code has no `crate::server`, `crate::lsp`, or `crate::features`
import. The structure check enforces this; the loader test modules that still
build their load through the server are named in its policy.

## Inputs

The server builds a `LoadContext` for each whole-project load
(`load_context_for_project`) and each interactive file pass
(`load_context_for_uri`), and passes it to
`IndexingCoordinator::run_complete_indexing` and the `FileProcessor::analyze_file*`
entry points. Its handles read live: configuration, published require roots,
and open buffers are observed when the loader consults them. The loader reads
configuration, require roots, shared products, the resource governor, runtime
discovery, and open buffers only through this context.

## Writes and owner lookups

`LoadContext::sink` is a `LoadSink`, implemented by the server in
`src/server/projects/load_sink.rs`. Every write and owner lookup the loader
makes goes through it as one domain operation: engine and project routing,
indexing run checks and phase transitions, progress, runtime, Ruby version,
and the JRuby add-on (`set_jruby_add_on`, one write that replaces
provider and classpath fingerprint together), source registration, processed-document marks,
require-root publication and require-diagnostic refresh, navigation demand
queues, inlay-hint refresh, and extension registry and context. The loader
calls them in its own order, so the owner observes the load's write sequence.
Engine reads and writes go through the `LoadTarget` that
`LoadSink::target_for_uri` returns for a project. The trait is defined here and
the server implements it for `ProjectHandle`; a scratch engine
(`parking_lot::RwLock<AnalysisEngine>`) implements it too, for semantic
contexts and dependency products the loader builds privately. The loader
writes only through the named operations on `dyn LoadTarget`: fact replacement
(`replace_file_facts`, `replace_facts_by_path`, and
`replace_facts_if_source_snapshot`, which checks the source snapshot and
replaces under one write guard), source registration (`register_*`, including
the snapshot-conditional batch `register_project_sources_if_snapshot`),
`commit_signature_source`, `commit_extension_seed`, `bind_gem_product`,
`install_template_if_empty`, `resolve`, `resolve_files`, and `compact`. Only
`target.rs` can construct the `NamedWrite` token the raw write primitive
requires, so no other loader module can take an engine write guard. Reads use
`view(|view| ..)`.

A single-file pass is split into analysis and commit. `FileProcessor::analyze_file*`
parses the file, walks it, and composes its `FileAnalysis`, writing only what
the walk itself reads: the source registration, the direct declaration seed,
and the extension seed through `commit_extension_seed`. It returns an
uncommitted `LoadedFile`. The caller commits it with `LoadedFile::commit`
immediately, in the same task and with no await in between: the commit
replaces the file's facts in the engine the analysis read, resolves them as
requested, classifies the semantic change, and retains the processed document
through `LoadSink::mark_document_indexed`. An open version that is already
indexed commits nothing; a file too broken to analyze commits empty facts.

The extension registry never writes an engine. Before a file walk the loader
asks it for the semantic seed (extension namespaces and method targets) that
the engine lacks for the project's applicability, and commits that seed with
the target's `commit_extension_seed`. The registry hands the seed over inside
its seed ledger lock and records it as applied only after the commit returns,
so concurrent seeds of one engine commit in ledger order and the engine always
holds the last recorded applicability. A runtime rebuild empties a project
engine in place, so the server tells the registry to forget that engine's seed
(`forget_semantic_seed`) and the rebuild seeds it again.

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
