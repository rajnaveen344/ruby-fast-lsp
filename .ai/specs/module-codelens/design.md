The CodeLens for modules is implemented by a provider that identifies module definitions and queries the index for usages of that module via `include`, `prepend`, and `extend`. The provider summarizes counts per mixin type and exposes a single action to open references.

## Overview

- Input: a Ruby document with one or more `module` declarations (including nested/namespaced forms).
- Output: CodeLens items above each module definition showing `X include | Y prepend | Z extend` (omitting zero-count categories).
- Trigger: on document open, change, and index updates.

## Responsibilities

- Discover module definitions in the current document and compute their fully-qualified names (FQNs).
- Query the global index for reverse mixin relationships (which classes/modules use the target module).
- Aggregate usage counts by mixin type and render the CodeLens label.
- Provide a command that opens the references view scoped to the module’s mixin usages.

## Module Identification

- Parse the AST for `Module` nodes and nested modules.
- Resolve FQN using the file’s namespace plus lexical parents (e.g., a `module Inner` inside `module Outer` resolves to `Outer::Inner`).
- For `Module.new` assigned to a constant, resolve the constant name as the module name (e.g., `MyModule = Module.new`).

## Index Integration

- Mixins (include/prepend/extend) are indexed with the callee (the module being used) and the caller (the class/module using it) along with the `MixinType`.
- The provider queries a reverse-mixins mapping keyed by the module FQN to retrieve a list of usages: `Vec<(FullyQualifiedName, MixinType)>`.
- Counts are computed by grouping usages by `MixinType`.

## Labeling and Actions

- Label formatting: `X include | Y prepend | Z extend`, omitting categories with zero counts.
- Single action: clicking the CodeLens opens the references panel filtered to these mixin calls.

## Update Strategy

- Compute lenses lazily when requested.
- Debounce on document edits to avoid churn.
- Invalidate and recompute when the index signals updated mixin data.

## Performance Considerations

- Name resolution is O(depth of lexical scope); mixin queries are O(k) where k is number of usages for the module.
- Avoid walking the entire workspace; rely on the index’s reverse-mixins structure.
- Cache FQN for module declarations per document to reduce duplicate work across requests.

## Failure Modes

- If the index is unavailable or incomplete, show no lens (or a subtle `indexing…` state if desired) and avoid exceptions.
- If FQN cannot be resolved (e.g., ambiguous dynamic assignment), skip CodeLens for that declaration.

## Future Extensions

- Optional category breakdown in the references view (tabs for include/prepend/extend).
- Optional configuration to include/exclude usages from external gems.
- Support for refinements (`using`) in a separate CodeLens category (out of scope for v1).

## Testing Plan (High-Level)

- Unit tests: FQN resolution for nested/namespaced modules and `Module.new` constants.
- Integration tests: usage counting across files; formatting rules; zero-count omission.
- Edge tests: conditional mixins within `if`/`case`, reopened modules, absolute vs. relative constants (`::A::B` vs `A::B`).