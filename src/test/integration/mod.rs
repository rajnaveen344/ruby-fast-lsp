//! Integration tests organized by tested entity.
//!
//! Structure:
//! - classes/ - Tests for classes (goto, references)
//! - constants/ - Tests for constants
//! - methods/ - Tests for methods (goto, references, inlay hints, inference)
//! - modules/ - Tests for modules (code lens for mixins)
//! - variables/ - Tests for variables (inlay hints)

mod classes;
mod constants;
mod methods;
mod modules;
mod variables;
