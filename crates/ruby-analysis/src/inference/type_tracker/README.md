# Type tracker

`TypeTracker` follows an existing Prism tree, derives local types, and records
evidence for the fact collector. It owns one inference pass, not indexed project
state. The engine retains file replacement, semantic lookup, and cross-file
solved outcomes.

## Start here

Read `mod.rs` for the seven state owners, then `traversal.rs` for statement order
and node dispatch. Follow a call into `expressions/`, a branch into `flow/`, or a
method solve into `returns/`.

Construct with `TypeTracker::new()`. The tracker takes Prism nodes in its
tracking methods and does not retain source bytes or a source lifetime. The
collector supplies method contracts, same-file evidence, and optional engine
queries through builders and context setters. Local tracking without an engine
remains supported; unavailable lookup evidence stays unproven.

| Entry point | Responsibility |
| --- | --- |
| `track_program` | Follow top-level statements |
| `track_method` / `track_method_outcome` | Infer explicit and fallthrough returns, retaining an Unknown reason when requested |
| `track_method_equation` (crate-private) | Collect a same-file return equation for engine resolution |
| `track_isolated_block_body` (crate-private) | Infer a block body with the supplied parameter types |
| `into_var_types` | Return variable snapshots for offset queries |
| `take_local_read_types` (crate-private) | Return exact reads, collapsing repeated loop visits to their final evidence |

## State ownership

All tracker fields are private. Helper visibility is restricted to this module
tree; splitting implementation files does not expose mutable state to callers.

| Field | Owner | What it keeps |
| --- | --- | --- |
| `environment` | `flow/environment.rs` | Current locals, callable bindings, correlated shape aliases and containment, and their proof metadata |
| `context` | `context.rs` | Parameter contracts and the current class and method |
| `analysis` | `context.rs` | Optional engine/query cache and supplied same-file lookup evidence |
| `returns` | `returns/mod.rs` | Private return terms, dependencies, explicit returns, and recursive approximation |
| `observations` | `observations.rs` | Offset snapshots and exact local-read evidence for publication |
| `control_flow` | `flow/mod.rs` | Loop bounds/depth and lexical rescue-entry accumulators |
| `next_shape_identity` | `mod.rs` | Fresh identities shared across branch traversal within one pass |

`FlowEnvironment` is one clone unit. Branches must copy its types, aliases,
containment, callable bindings, and proof metadata together. Its explicit
`types` view supports reads; environment mutation helpers maintain the related
metadata. Do not restore implicit dereferencing to the underlying map.

The shape identity allocator deliberately lives outside those clones: separate
branches must not allocate colliding identities. Each method pass resets it at
the same boundary as the environment. Flow identities never enter engine facts;
published results contain canonical `RubyType` values and proof reasons.

## Implementation map

| Area | Files and responsibilities |
| --- | --- |
| Root | State wiring, supplied context, recorded observations, and Prism traversal |
| `expressions/` | Expression/literal inference, method calls, callable/block inference, and constant lookup |
| `flow/` | Environment joins, branches, loops, rescue paths, pattern capture, and ordinary narrowing |
| `flow/shapes/` | Shape identities, canonical value operations, alias/containment tracking, mutation, and shape narrowing |
| `returns/` | Method passes, bounded recursive solving, and returned dependency terms |
| `tests/` | Assignments, returns, control flow, shape values, array aliases, shape aliases, and shape narrowing |

Every directory here stays within the repository limit of ten immediate files
and subfolders. Add behavior beside its semantic responsibility; create another
meaningful group before a folder exceeds the limit.

## Preserving behavior

Loop stabilization remains bounded at ten iterations by default; only the
outer loop repeats stabilization, while nested loops receive one pass per outer
visit. Direct recursive returns use the existing bounded solver and its private
bottom approximation. Shape bounds and Unknown propagation are unchanged.

The nested-loop regression counts ordinary exact local-read observations before
publication collapses them. It exercises the production traversal and verifies
linear visits without a test-only counter or an alternate execution path.

Use the focused tracker suite while changing local inference, then the workspace
suite for collector, engine, editor, and simulation consumers. Keep branch clone
boundaries, method reset order, and return dependency publication explicit when
adding state.
