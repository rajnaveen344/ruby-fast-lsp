//! Integration tests organized by feature.

// Feature-based organization
mod call_hierarchy;
mod code_lens;
mod completion;
mod diagnostics;
mod document_highlights;
mod extensions;
mod folding_range;
mod formatting;
mod goto;
mod hover;
mod implementation;
mod inference;
mod inlay_hints;
mod mixins;
mod references;
mod rename;
mod selection_ranges;
mod signature_help;
mod type_hierarchy;

// Domain-specific (YARD type annotations)
mod constants;

// Multi-workspace routing
mod workspaces;
