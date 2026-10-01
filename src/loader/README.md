# Loader

The loader layer discovers Ruby files, parses them, and feeds analysis facts
into `ruby-analysis::engine`.

Storage is not owned here. The engine owns symbols, methods, types, graph facts,
reference candidates, resolved references, and diagnostics.

## Main Pieces

- `coordinator/`: workspace indexing orchestration, scheduling priority,
  runtime selection, JRuby companions, and resource admission
- `file_processor/`: parse one file and run `FactCollector`, merge collected
  facts, and convert extension-produced facts
- `require_paths/`: require-path resolution
- `sources/project/`: project root discovery, project file discovery,
  navigation-demand collection, and dependency scan
- `sources/stdlib/`: standard library file discovery and exact runtime load paths
- `sources/gems/`: gem discovery, lockfile selection, vendor cache extraction,
  and shared gem dependency products

See [namespace indexing](../../docs/development/namespace-indexing.md) for how
constant-path module and class definitions map to namespaces.

## Current Flow

1. Scan project dependencies.
2. Collect facts from gems.
3. Collect facts from stdlib.
4. Collect facts from project files.
5. Publish engine diagnostics.

`FactCollector` emits reference candidates during the same pass as definitions.
The engine resolves candidates after each file update.
