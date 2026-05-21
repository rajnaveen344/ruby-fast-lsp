//! Hover Query - Unified hover information retrieval.
//!
//! This module provides hover information using a clean architecture:
//! 1. Get identifier at position (via RubyPrismAnalyzer)
//! 2. Convert Identifier → HoverTarget (analysis-domain representation)
//! 3. Generate hover content (via generator functions)

pub mod generators;

use generators::HoverContext;
pub use generators::HoverInfo;

use crate::query::EngineQuery;
use ruby_analysis::indexer::{identifier_to_hover_target, HoverTarget, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Position, Url};

impl EngineQuery {
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
        let (identifier_opt, identifier_type, namespace, scope_id, _namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        // Step 2: Convert Identifier to HoverTarget (analysis-domain representation)
        let target =
            identifier_to_hover_target(identifier, identifier_type, namespace, scope_id, position);

        // Step 3: Create context for generators
        let context = HoverContext {
            document: self.doc.as_ref(),
            analysis_engine: self.analysis_engine.as_ref(),
        };

        // Step 4: Generate hover content based on node type
        match target {
            HoverTarget::LocalVariable { .. } => {
                generators::generate_local_variable_hover(&target, &context)
            }
            HoverTarget::Constant { .. } => generators::generate_constant_hover(&target, &context),
            HoverTarget::Method { .. } => generators::generate_method_hover(&target, &context),
            HoverTarget::InstanceVariable { .. }
            | HoverTarget::ClassVariable { .. }
            | HoverTarget::GlobalVariable { .. } => {
                generators::generate_variable_hover(&target, &context)
            }
            HoverTarget::YardType { .. } => generators::generate_yard_type_hover(&target),
        }
    }
}
