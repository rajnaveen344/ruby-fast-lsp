---
name: architecture
description: "Design Ruby Fast LSP changes using the current ruby-analysis core/engine/indexer/inference boundaries."
---

# Architecture

Use this skill for structural changes, module placement, dependency direction, or questions about where logic belongs.

## Source Of Truth

Read `AGENTS.md` first. It contains the detailed current architecture direction. Treat this skill as a compact checklist, not a replacement.

## Current Boundary

`ruby-fast-lsp` should stay a thin LSP/editor adapter over reusable analysis crates.

| Layer                      | Owns                                                                                                   | Must Not Own                                             |
| -------------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| `ruby-analysis::core`      | FQNs, Ruby names, ranges, source IDs, facts, Ruby types                                                | AST traversal, query policy, LSP/editor protocol         |
| `ruby-analysis::engine`    | Workspace semantic state, fact ingestion, graph/reference/diagnostic resolution, deterministic queries | `tower_lsp` types, snippets, editor triggers             |
| `ruby-analysis::indexer`   | Ruby parsing and AST traversal that emits facts/candidates                                             | Global semantic truth, LSP protocol, workspace lifecycle |
| `ruby-analysis::inference` | Type derivation, local flow tracking, RBS lookup/substitution                                          | LSP protocol, editor UX, persistent workspace ownership  |
| `src/*`                    | Server lifecycle, document cache, handlers, capabilities, protocol conversion                          | Reusable type/graph algorithms                           |
| `extensions/*`             | External DSL/library facts and patches                                                                 | Global source of truth                                   |

## Placement Rules

- If code consumes or returns `tower_lsp::lsp_types::*`, `Url`, snippets, trigger characters, editor commands, or diagnostics publishing, keep it in `src/`.
- If code consumes or returns `TextRange`, FQNs, facts, graph entries, or `RubyType`, put it in `crates/ruby-analysis`.
- `src/query/*` is an adapter over `ruby-analysis::engine::AnalysisQuery`; it may map cursor/document context to domain queries and map `TextRange` back to LSP `Location`.
- Method lookup semantics must stay single-sourced in engine resolution. Use `AnalysisQuery::resolve_method_callees*` for navigation and `AnalysisQuery::resolve_method_reference*` for reference/diagnostic policy.
- Do not reintroduce public store getters or public `HashMap<FullyQualifiedName, Vec<Fact>>` data access.

## Library API

The crate root exposes only `core`, `engine`, `indexer`, and `inference`.
Import `RubyType` from `core`; use `engine::AnalysisQuery` or file-scoped
`engine::TypeQuery` for reads. Core implementation modules, stores, and interned
representations stay crate-private. Extension collectors expose fact operations,
not a mutable store handle. `FactCollector` keeps traversal state private and
returns `CollectedFile` through `finish()` for production and simulation file
composition. Keep new state and helpers beside their collector responsibility;
see `crates/ruby-analysis/src/indexer/fact_collector/README.md` and the library
guide at `crates/ruby-analysis/README.md`.

Engine resolution coordinates inference's AST-free constant and method-return
equation solvers. Inference may consult engine queries; engine retains ownership
of lookup policy, file replacement, and solved state. These are cooperating
modules in one crate, not a strictly acyclic set of crate dependencies.

## Engine Write Path

Use one write path:

```rust
let file_id = engine.register_file(input);
let facts = collect_facts(file_id, &content);
engine.replace_facts(file_id, facts, ResolveMode::Immediate);
```

For workspace indexing, defer resolution per file and call `engine.resolve()` once after the batch.

## Review Questions

- Can this logic be used by a non-LSP client? If yes, it probably belongs in `ruby-analysis`.
- Does this duplicate method/MRO/diagnostic policy already owned by engine resolution?
- Does the proposed API expose store internals rather than domain views or query primitives?
- Does this change keep parsing/fact collection separate from semantic graph ownership?
