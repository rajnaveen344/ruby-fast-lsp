# Query Engine Tasks

## Task Status Overview

### ✅ Completed Tasks
- [x] Requirements analysis
- [x] Architecture design
- [x] Capability categorization (index-heavy vs AST-only)
- [x] **Phase 1.1** — Foundation: `IndexQuery` struct, factory methods
- [x] **Phase 1.2** — Definition queries (`query/definition.rs`)
- [x] **Phase 1.3** — Reference queries (`query/references.rs`)
- [x] **Phase 1.4** — Type queries (`query/types.rs`)
- [x] **Phase 1.5** — Method resolution (`query/method.rs`)
- [x] **Phase 1.6** — Hover queries (`query/hover.rs`)
- [x] **Phase 1.7** — Completion queries (`query/completion.rs`)
- [x] **Phase 1.8** — Diagnostics queries (`query/diagnostics.rs`)
- [x] **Phase 1.9** — Code lens queries (`query/code_lens.rs`)
- [x] **Phase 1.10** — Workspace symbols queries (`query/workspace_symbols.rs`)
- [x] **Phase 1.11** — Inlay hints queries (`query/inlay_hints.rs`)
- [x] **Phase 1.12** — Namespace tree queries (`query/namespace_tree.rs`)
- [x] **Phase 1.13** — Type hierarchy queries (`query/type_hierarchy.rs`)
- [x] **Phase 1.14** — Inference queries (`query/inference.rs`)
- [x] **Phase 1.15** — Debug queries (`query/debug.rs`)
- [x] **Phase 2** — All capability handlers updated to use IndexQuery
- [x] **Phase 3.2** — Documentation updated

### 🔄 In Progress Tasks
None currently.

### 📋 Pending Tasks

#### Phase 3: Cleanup (Optional)

- [ ] **Remove dead code from capabilities**
  - Some capability files still contain private helpers that were copied (not moved) to query/.
  - Audit and remove any truly dead code.
  - Keep AST-only features in capabilities: folding, semantic_tokens, document_symbols, formatting.

## Query Module Summary

All query modules are now implemented in `src/query/`:

| Module | File | Methods |
|--------|------|---------|
| Foundation | `mod.rs` | `IndexQuery::new`, `with_doc`, `with_uri` |
| Definition | `definition.rs` | `find_definitions_at_position` |
| References | `references.rs` | `find_references_at_position` |
| Hover | `hover.rs` | `get_hover_for_position` |
| Completion | `completion.rs` | `find_constant_completions`, `find_method_completions` |
| Debug | `debug.rs` | `debug_lookup`, `debug_stats`, `debug_ancestors`, `debug_methods`, `debug_inference_stats`, `debug_export_graph` |
| Method | `method.rs` | Method resolution and return types |
| Types | `types.rs` | Type inference utilities |
| Diagnostics | `diagnostics.rs` | YARD and unresolved diagnostics |
| Code Lens | `code_lens.rs` | `get_code_lenses` |
| Workspace Symbols | `workspace_symbols.rs` | `get_top_level_symbols`, `search_workspace_symbols` |
| Inlay Hints | `inlay_hints.rs` | Inlay hint generation |
| Namespace Tree | `namespace_tree.rs` | Namespace tree queries |
| Type Hierarchy | `type_hierarchy.rs` | Supertype/subtype queries |
| Inference | `inference.rs` | `ReceiverResolver`, `ReturnTypeResolver`, `LocalVariableResolver` |

## Verification

All verified:
- [x] `cargo build` — zero compilation errors
- [x] `cargo test` — 702 tests pass, 0 failures
- [x] All capabilities route through `IndexQuery`

## Success Criteria

- [x] All index-heavy logic in `src/query/`
- [x] `capabilities/` are thin adapters over query layer
- [x] All existing tests pass
- [x] No performance regression
