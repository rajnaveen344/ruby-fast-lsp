//! Implementation Query - Find where methods/modules are concretely implemented
//!
//! Answers "textDocument/implementation":
//! - For a method: find all overrides in descendant classes and including classes
//! - For a module/class: find all classes that include/prepend/extend it

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use log::info;
use ruby_analysis::engine::AnalysisQuery;
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::location_for_range;
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
                self.find_method_implementations(&owner_fqn, iden)
            }
            Identifier::RubyConstant { namespace: _, iden } => {
                let fqn = self.resolve_constant_fqn(iden, &ancestors);
                self.find_namespace_implementations(&fqn)
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

    /// Find all implementations of a method across descendants and includers.
    ///
    /// Given `Serializable#to_json`, finds overrides in:
    /// 1. Direct descendants (subclasses, sub-subclasses, etc.)
    /// 2. Transitive mixers (modules/classes that include/prepend, recursively)
    /// 3. Descendants of each mixer
    ///
    /// Example: Module A included by Module B included by Class C < Class D
    /// → checks B, C, and D for method overrides.
    fn find_method_implementations(
        &self,
        owner_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        self.method_implementations_from_analysis(owner_fqn, method)
    }

    /// Find all implementations of a module/class.
    ///
    /// For a module: returns all classes/modules that include/prepend/extend it
    /// (transitively through module chains), plus all subclasses.
    /// For a class: returns all subclasses.
    fn find_namespace_implementations(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        self.namespace_implementations_from_analysis(fqn)
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
        let locations = query
            .method_implementation_ranges(owner_fqn, method)
            .into_iter()
            .filter_map(|range| location_for_range(&engine, range))
            .collect::<Vec<_>>();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
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
        let locations = query
            .namespace_implementation_ranges(fqn)
            .into_iter()
            .filter_map(|range| location_for_range(&engine, range))
            .collect::<Vec<_>>();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }
}
