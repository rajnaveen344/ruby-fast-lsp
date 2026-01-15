//! AST node types collected by InlayNodeCollector for inlay hint generation.
//!
//! These types represent the raw AST data needed to generate hints.
//! The collector only extracts nodes; hint generation is handled separately.

use tower_lsp::lsp_types::Position;

/// Represents nodes collected from AST that are relevant for inlay hints.
///
/// The collector extracts these nodes during AST traversal.
/// Generators then convert them to actual hints.
#[derive(Debug)]
pub enum InlayNode {
    /// Block end: class/module/def for end labels
    BlockEnd {
        kind: BlockKind,
        name: String,
        end_position: Position,
    },

    /// Variable assignment for type hints
    VariableWrite {
        kind: VariableKind,
        name: String,
        name_end_position: Position,
    },

    /// Method definition for return type and parameter hints
    MethodDef {
        name: String,
        params: Vec<ParamNode>,
        return_type_position: Position,
    },

    /// Chained method call with line break for intermediate type hints
    /// TODO: Add method_name and receiver_offset when implementing type inference
    ChainedCall { call_end_position: Position },

    /// Implicit return in method body
    ImplicitReturn { position: Position },
}

/// The kind of block for end labels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Class,
    Module,
    Method,
}

impl BlockKind {
    /// Returns the keyword for this block kind (used in end labels)
    pub fn keyword(&self) -> &'static str {
        match self {
            BlockKind::Class => "class",
            BlockKind::Module => "module",
            BlockKind::Method => "def",
        }
    }
}

/// The kind of variable for type hints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Local,
    Instance,
    Class,
    Global,
    Constant,
}

/// A method parameter node
#[derive(Debug)]
pub struct ParamNode {
    pub name: String,
    pub end_position: Position,
    /// Whether this is a keyword parameter (has colon in syntax)
    pub has_colon: bool,
}
