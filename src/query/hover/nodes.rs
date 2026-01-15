//! Hover node types - simplified representations of AST nodes for hover generation.
//!
//! These types represent what was found at a hover position.
//! They are simpler than the `Identifier` enum and contain only the data needed
//! for generating hover information.

use crate::analyzer_prism::MethodReceiver;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::scope::LVScopeId;
use tower_lsp::lsp_types::Position;

/// Represents a node at the hover position.
///
/// This is a simplified version of `Identifier` that contains only
/// the essential data needed for hover generation.
#[derive(Debug, Clone)]
pub enum HoverNode {
    /// Local variable at hover position
    LocalVariable {
        name: String,
        position: Position,
        scope_id: LVScopeId,
    },

    /// Constant (class/module) at hover position
    Constant {
        /// The constant path (e.g., ["Foo", "Bar"])
        path: Vec<RubyConstant>,
    },

    /// Method at hover position
    Method {
        name: String,
        position: Position,
        receiver: MethodReceiver,
        namespace: Vec<RubyConstant>,
        scope_id: LVScopeId,
        /// True if this is a method definition (def), false if method call
        is_definition: bool,
    },

    /// Instance variable at hover position
    InstanceVariable { name: String },

    /// Class variable at hover position
    ClassVariable { name: String },

    /// Global variable at hover position
    GlobalVariable { name: String },

    /// YARD type reference in documentation
    YardType { type_name: String },
}
