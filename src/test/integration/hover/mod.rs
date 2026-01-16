//! Hover integration tests - organized by AST node type.
//!
//! Structure:
//! - local_variable/ - Hover on local variable reads
//! - call_node/ - Hover on method calls (with/without receiver)
//! - constant/ - Hover on class/module references

pub mod call_node;
pub mod constant;
pub mod local_variable;
