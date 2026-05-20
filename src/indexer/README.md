# Indexer

The indexer layer discovers Ruby files, parses them, and feeds analysis facts
into `ruby-analysis-engine`.

Storage is not owned here. The engine owns symbols, methods, types, graph facts,
reference candidates, resolved references, and diagnostics.

## Main Pieces

- `coordinator.rs`: workspace indexing orchestration
- `file_processor.rs`: parse one file and run `FactCollector`
- `indexer_project.rs`: project file discovery and dependency scan
- `indexer_stdlib.rs`: standard library file discovery
- `indexer_gem.rs`: gem file discovery

## Current Flow

1. Scan project dependencies.
2. Collect facts from gems.
3. Collect facts from stdlib.
4. Collect facts from project files.
5. Publish engine diagnostics.

`FactCollector` emits reference candidates during the same pass as definitions.
The engine resolves candidates after each file update.
