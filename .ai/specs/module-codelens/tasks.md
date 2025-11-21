# Module CodeLens — Implementation Tasks

## Core Implementation

- Implement module CodeLens provider scaffolding (request handling and lens assembly).
- Discover module declarations in a document and compute fully-qualified names.
- Query index for reverse mixins per module FQN; aggregate counts by type.
- Format CodeLens labels (`X include | Y prepend | Z extend`); omit zero categories.
- Provide a single command to open references for all mixin usages of the module.

## Integration

- Hook provider to document open/change events; debounce recomputation.
- Subscribe to index updates to invalidate cached results and recompute lenses.
- Respect global and per-feature settings (`rubyFastLSP.codeLens.modules.enabled`).

## Edge Handling

- Handle nested/namespaced modules and reopened modules (aggregate by FQN).
- Support `Module.new` assigned to a constant as module name.
- Honor absolute vs. relative constants (e.g., `::A::B` vs `A::B`).
- Skip dynamic mixins (`send`, `const_get`) and refinements (`using`) in v1.

## Performance

- Cache module FQN resolution per document.
- Avoid redundant index queries; batch per document when possible.
- Ensure lens computation is fast and non-blocking; debounce edits.

## Testing

- Unit: FQN resolution (nested, namespaced, `Module.new`), label formatting.
- Integration: counts across files; omission of zero categories; references command opens expected sites.
- Edge: conditional mixins inside control flow; reopened modules; absolute vs. relative constants.

## Milestones

- M1: Provider skeleton and single-file `include` counts.
- M2: Support `prepend` and `extend`; cross-file counts via index.
- M3: Name resolution polish; nested/namespaced modules; `Module.new` constants.
- M4: Debounce + index update wiring; configuration toggle.
- M5: Test suite coverage and documentation polish.