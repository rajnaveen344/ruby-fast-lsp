//! Integration tests grouped by the same feature families as `src/features/`.

// Diagnostics publication and diagnostic kinds.
mod diagnostics;
// Rename, completion, formatting, and signature help.
mod editing;
// Open/edit/close, background schedules, and dependency refresh ordering.
mod lifecycle;
// Definition, references, hierarchies, and highlights.
mod navigation;
// Hover, inlay hints, code lenses, folding, and selection ranges.
mod presentation;
// Cross-cutting Ruby semantics: inference, constants, and mixins.
mod semantics;
// Source kinds and project ownership: ERB, extensions, and workspaces.
mod sources;
