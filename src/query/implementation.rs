//! Implementation Query - Find where methods/modules are concretely implemented
//!
//! Answers "textDocument/implementation":
//! - For a method: find all overrides in descendant classes and including classes
//! - For a module/class: find all classes that include/prepend/extend it

use log::info;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::{Identifier, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::{locations_for_ranges, non_empty_locations};
use super::EngineQuery;

impl EngineQuery {
    /// Find implementations for the identifier at the given position.
    ///
    /// - Cursor on a method definition → find all overrides in descendants/includers
    /// - Cursor on a class/module name → find all classes that include/prepend/extend it,
    ///   plus all subclasses
    pub fn find_implementations_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier, _, ancestors, _scope_stack, namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = match identifier {
            Some(id) => id,
            None => {
                info!("No identifier found at position {:?}", position);
                return None;
            }
        };

        info!(
            "Looking for implementations of: {}->{}",
            FullyQualifiedName::from(ancestors.clone()),
            identifier,
        );

        match &identifier {
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => {
                // Resolve the owner class/module of this method
                let owner_fqn = self.resolve_receiver_to_namespace(
                    receiver,
                    &ancestors,
                    namespace_kind,
                    position,
                )?;
                self.method_implementations_from_analysis(&owner_fqn, iden)
            }
            Identifier::RubyConstant { namespace: _, iden } => {
                let fqn = self.resolve_constant_fqn(iden, &ancestors);
                self.namespace_implementations_from_analysis(&fqn)
            }
            _ => {
                info!(
                    "Implementation not supported for identifier type: {:?}",
                    identifier
                );
                None
            }
        }
    }

    fn method_implementations_from_analysis(
        &self,
        owner_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: method implementation query requires an analysis engine. \
             This is a bug because LSP implementation should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_doc_and_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.method_implementation_ranges(owner_fqn, method),
        ))
    }

    fn namespace_implementations_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: namespace implementation query requires an analysis engine. \
             This is a bug because LSP implementation should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_doc_and_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.namespace_implementation_ranges(fqn),
        ))
    }
}
