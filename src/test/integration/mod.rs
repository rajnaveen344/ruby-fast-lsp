//! Integration tests organized by tested entity and feature.
//!
//! ## By Feature (preferred for hover/inlay_hints)
//! - hover/ - Hover tests organized by AST node type
//! - inlay_hints/ - Inlay hints tests organized by hint type
//!
//! ## By Entity (legacy organization)
//! - classes/ - Tests for classes (goto, references)
//! - constants/ - Tests for constants
//! - methods/ - Tests for methods (goto, references, inference)
//! - modules/ - Tests for modules (code lens for mixins)
//! - variables/ - Tests for variables

// Feature-based organization (new)
mod hover;
mod inlay_hints;

// Entity-based organization (legacy)
mod classes;
mod constants;
mod methods;
mod modules;
mod variables;
