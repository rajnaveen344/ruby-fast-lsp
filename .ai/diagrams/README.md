# Ruby Fast LSP - C4 Architecture Diagrams

This directory contains [LikeC4](https://likec4.dev/) diagrams that document the architecture of Ruby Fast LSP.

## File Structure

| File          | Description                                                                                                     |
| ------------- | --------------------------------------------------------------------------------------------------------------- |
| `model.c4`    | **Base model.** Defines specification, actors, external systems, and Level 1-2 diagrams (Context & Containers). |
| `server.c4`   | **Server components.** Level 3 details for LSP Server, Query Layer, Index, and Docs.                            |
| `indexing.c4` | **Indexing lifecycle.** Indexer and Analyzer components with a dynamic view of the indexing process.            |
| `requests.c4` | **Request flow.** Dynamic view showing how an LSP request (e.g., Go to Definition) flows through the system.    |

## C4 Levels

1. **Level 1 - System Context**: Shows Ruby Fast LSP and its external dependencies (IDE, Prism, File System).
2. **Level 2 - Containers**: Shows the main containers within the LSP (Server, Index, Docs, Query).
3. **Level 3 - Components**: Shows the internal components of each container.

## Key Concepts

### Containers

- **LSP Server**: Handles LSP protocol via `tower-lsp`. Routes requests to handlers.
- **Ruby Index**: Stores all cross-file symbols (Class, Module, Method, Constant, ClassVar, InstanceVar, GlobalVar).
- **Document Cache**: Stores `RubyDocument` objects with local variable symbols for open files.
- **Query Layer**: Provides a clean API for handlers to query Index and Docs.

### Indexing Flow

1. IDE sends `initialized` notification
2. Server spawns background indexing task
3. **Phase 1**: Index definitions from project files, stdlib, and gems
4. Resolve inheritance and mixins
5. **Phase 2**: Index references from project files
6. Build completion trie
7. Server is ready for requests

## Viewing the Diagrams

Install the [LikeC4 VS Code extension](https://marketplace.visualstudio.com/items?itemName=likec4.likec4) to preview diagrams directly in the editor.

Or run locally:

```bash
npx likec4 serve .ai/diagrams
```
