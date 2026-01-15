# Ruby Fast LSP - C4 Architecture Diagrams

This directory contains [LikeC4](https://likec4.dev/) diagrams that document the architecture of Ruby Fast LSP.

## File Structure

| File                  | Description                                                                                                     |
| --------------------- | --------------------------------------------------------------------------------------------------------------- |
| `model.c4`            | **Base model.** Defines specification, actors, external systems, and Level 1-2 diagrams (Context & Containers). |
| `server.c4`           | **Server components.** Level 3 details for LSP Server, Query Layer, Index, and Docs.                            |
| `indexing.c4`         | **Indexing lifecycle.** Indexer and Analyzer components with a dynamic view of the indexing process.            |
| `requests.c4`         | **Request flow.** Dynamic view showing how an LSP request (e.g., Go to Definition) flows through the system.    |
| `notifications.c4`    | **Notification flow.** Dynamic views for LSP notifications (didOpen, didChange, didSave, etc.).                  |
| `inlay_hints.c4`      | **Inlay hints flow.** Dynamic views showing request flow, internal architecture, and on-demand type inference.   |
| `type_narrowing.c4`   | **Type narrowing & CFG.** CFG building, dataflow analysis, type guard application, and capability integration.   |

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

### Inlay Hints Architecture

The inlay hints implementation follows a clean, principled architecture within the `InlayHintsQuery` component:

1. **LSP Request**: IDE sends `textDocument/inlayHint` with URI and range
2. **Capability Handler** (thin): Delegates to query layer
3. **Query Layer** (`InlayHintsQuery`):
   - Infers return types for visible methods (on-demand)
   - Parses AST via Prism
   - Runs `InlayNodeCollector` (visitor) to collect relevant nodes
   - Passes nodes to generators with context
4. **Internal Components** (within `query/inlay_hints/`):
   - `collector.rs`: Visitor that collects `InlayNode` instances
     - `BlockEnd` (class/module/def end labels)
     - `VariableWrite` (local, instance, class, global variables)
     - `MethodDef` (method definitions with params)
     - `ImplicitReturn` (implicit returns in methods)
     - `ChainedCall` (method chains with line breaks)
   - `nodes.rs`: Data structures representing collected AST nodes
   - `generators.rs`: Convert nodes to hints with type inference
     - `generate_structural_hints()` - end labels, implicit returns
     - `generate_variable_type_hints()` - variable types from inference
     - `generate_method_hints()` - return types, parameter types (YARD)
5. **Response**: Convert to LSP `InlayHint` and return to IDE

**Key Principles:**
- **No ad-hoc processing**: Only uses AST nodes from visitor
- **Clear separation**: Collector collects, generators generate
- **On-demand computation**: Hints and types computed fresh on each request
- **Range-based filtering**: Only processes nodes in requested range
- **Lazy type inference**: Method return types inferred only for visible methods

### Type Narrowing & CFG Architecture

The type narrowing system uses Control Flow Graph (CFG) analysis to provide precise type inference:

1. **Type Narrowing Query**: Capability (hover, completion) requests narrowed type at position
2. **CFG Building**: Ruby AST converted to control flow graph with type guards
3. **Dataflow Analysis**: Types propagated through blocks, guards applied at conditionals
4. **Type Guard Application**: Conditionals (is_a?, nil?, etc.) narrow types on branches
5. **Document Lifecycle**: Engine tracks file open/change/close, invalidates cached CFGs
6. **Capability Integration**: Hover and completion use narrowed types for better accuracy

**Key Components** (`inferrer/cfg/`):
- `engine.rs`: TypeNarrowingEngine - main API with lazy analysis and caching
- `builder.rs`: CFG Builder - converts AST to control flow graph
- `dataflow.rs`: Dataflow Analyzer - propagates types through CFG blocks
- `graph.rs`: CFG data structures - BasicBlock, edges, statements
- `guards.rs`: Type guards - IsA, IsNil, NotNil, RespondsTo, CaseMatch

**How It Works:**
- **Lazy Analysis**: Only analyzes methods containing queried positions
- **Type Guards**: Detect from conditionals (`if x.is_a?(String)` → `TypeGuard::IsA`)
- **Guard Application**: True branch narrows to type, false branch applies inverse
- **Type Merging**: Join points create unions (`String | NilClass`)
- **Per-Statement Snapshots**: O(log n) position lookup via binary search

**Example:**
```ruby
def process(value)        # value: Unknown
  if value.nil?           # Guard: IsNil(value)
    puts "nil"            # value: NilClass
  else                    # Guard: NotNil(value)
    value.upcase          # value: Unknown \ NilClass
  end
end
```

## Viewing the Diagrams

Install the [LikeC4 VS Code extension](https://marketplace.visualstudio.com/items?itemName=likec4.likec4) to preview diagrams directly in the editor.

Or run locally:

```bash
npx likec4 serve .ai/diagrams
```
