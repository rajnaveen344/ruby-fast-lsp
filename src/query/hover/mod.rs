//! Hover Query - Unified hover information retrieval.
//!
//! This module provides hover information using a clean architecture:
//! 1. Get identifier at position (via RubyPrismAnalyzer)
//! 2. Convert Identifier → HoverNode (simpler representation)
//! 3. Generate hover content (via generator functions)
//!
//! Architecture:
//! - `nodes.rs`: HoverNode data structures
//! - `generators.rs`: Pure functions that generate hover content
//! - `mod.rs` (this file): Orchestration layer

pub mod generators;
pub mod nodes;

pub use generators::HoverInfo;
use generators::HoverContext;
use nodes::HoverNode;

use crate::analyzer_prism::{Identifier, IdentifierType, RubyPrismAnalyzer};
use crate::query::IndexQuery;
use tower_lsp::lsp_types::{Position, Url};

impl IndexQuery {
    /// Get hover info for the symbol at position.
    ///
    /// This is the unified entry point for hover requests. It handles:
    /// - Local variables (with type inference from TypeQuery, document lvars, type snapshots)
    /// - Instance/class/global variables
    /// - Constants (classes, modules)
    /// - Methods (with receiver type resolution and return type inference)
    /// - YARD type references
    pub fn get_hover_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<HoverInfo> {
        // Step 1: Get identifier at position using existing analyzer
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, identifier_type, namespace, scope_id) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        // Step 2: Convert Identifier to HoverNode (simpler representation)
        let node = identifier_to_hover_node(identifier, identifier_type, namespace, scope_id, position)?;

        // Step 3: Create context for generators
        let context = HoverContext {
            index: self.index.clone(),
            uri,
            content,
            document: self.doc.as_ref(),
        };

        // Step 4: Generate hover content based on node type
        match node {
            HoverNode::LocalVariable { .. } => {
                generators::generate_local_variable_hover(&node, &context)
            }
            HoverNode::Constant { .. } => {
                generators::generate_constant_hover(&node, &context)
            }
            HoverNode::Method { .. } => {
                generators::generate_method_hover(&node, &context)
            }
            HoverNode::InstanceVariable { .. }
            | HoverNode::ClassVariable { .. }
            | HoverNode::GlobalVariable { .. } => {
                generators::generate_variable_hover(&node, &context)
            }
            HoverNode::YardType { .. } => {
                generators::generate_yard_type_hover(&node)
            }
        }
    }
}

/// Convert an Identifier to a HoverNode.
///
/// This simplifies the identifier representation to only what's needed for hover generation.
fn identifier_to_hover_node(
    identifier: Identifier,
    identifier_type: Option<IdentifierType>,
    namespace: Vec<crate::types::ruby_namespace::RubyConstant>,
    scope_id: crate::types::scope::LVScopeId,
    position: Position,
) -> Option<HoverNode> {
    match identifier {
        Identifier::RubyLocalVariable { name, .. } => Some(HoverNode::LocalVariable {
            name,
            position,
            scope_id,
        }),

        Identifier::RubyConstant { iden, .. } => Some(HoverNode::Constant { path: iden }),

        Identifier::RubyMethod {
            iden,
            receiver,
            namespace: method_ns,
        } => {
            let method_name = iden.to_string();
            let is_method_definition = identifier_type == Some(IdentifierType::MethodDef);

            // Use method_ns if available, otherwise fall back to namespace from analyzer
            let ns = if method_ns.is_empty() {
                namespace
            } else {
                method_ns
            };

            Some(HoverNode::Method {
                name: method_name,
                position,
                receiver,
                namespace: ns,
                scope_id,
                is_definition: is_method_definition,
            })
        }

        Identifier::RubyInstanceVariable { name, .. } => {
            Some(HoverNode::InstanceVariable { name })
        }

        Identifier::RubyClassVariable { name, .. } => Some(HoverNode::ClassVariable { name }),

        Identifier::RubyGlobalVariable { name, .. } => Some(HoverNode::GlobalVariable { name }),

        Identifier::YardType { type_name, .. } => Some(HoverNode::YardType { type_name }),
    }
}
