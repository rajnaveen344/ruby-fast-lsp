//! Reference Query - Find usages of symbols
//!
//! Consolidates reference logic from `capabilities/references.rs`.

use log::info;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::indexer::yard::YardTypeConverter;
use ruby_analysis::indexer::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::{locations_for_ranges, non_empty_locations};
use super::EngineQuery;

impl EngineQuery {
    /// Find all references to the symbol at the given position.
    pub fn find_references_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, _, ancestors, _scope_stack, namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        self.find_references_for_identifier(&identifier, &ancestors, namespace_kind, position)
    }

    /// Find references to a constant by FQN.
    fn find_constant_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.reference_locations_for_fqn_from_analysis(fqn) {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a variable (instance, class, or global).
    fn find_variable_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.variable_reference_locations_from_analysis(fqn) {
            info!("Found {} variable references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a method.
    ///
    /// Uses the same type-inference-based receiver resolution as go-to-definition
    /// to correctly resolve expression receivers. If the receiver type cannot be
    /// inferred, returns None rather than guessing (correctness over completeness).
    fn find_method_references(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        // `def initialize` is indexed as `new` (singleton) — map accordingly
        if method.as_str() == "initialize" {
            if let Ok(new_method) = RubyMethod::new("new") {
                let namespace_fqn = FullyQualifiedName::namespace_with_kind(
                    ancestors.to_vec(),
                    NamespaceKind::Singleton,
                );
                return self.method_reference_locations_for_namespace_from_analysis(
                    &namespace_fqn,
                    &new_method,
                );
            }
        }

        match receiver {
            MethodReceiver::Constant(receiver_ns) => self
                .method_reference_locations_for_constant_receiver_from_analysis(
                    receiver_ns,
                    ancestors,
                    method,
                ),
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                self.method_reference_locations_for_current_scope_from_analysis(ancestors, method)
            }
            // For expression receivers, use type inference to resolve the actual type.
            // This mirrors go-to-definition's `resolve_receiver_to_namespace`.
            _ => {
                let resolved_ns = self.resolve_receiver_to_namespace(
                    receiver,
                    ancestors,
                    namespace_kind,
                    position,
                )?;
                self.method_reference_locations_for_namespace_from_analysis(&resolved_ns, method)
            }
        }
    }

    /// Find references to a local variable using VariableScopes.
    fn find_local_variable_references(
        &self,
        name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();

        let byte_offset = document.position_to_analysis_offset(position);
        let ranges = document.local_variable_reference_ranges_at(name, byte_offset);
        if ranges.is_empty() {
            return None;
        }

        Some(
            ranges
                .into_iter()
                .map(|range| document.text_range_to_lsp_location(range))
                .collect(),
        )
    }
}

// Private helpers
impl EngineQuery {
    /// Find references for a given identifier.
    fn find_references_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                self.constant_reference_locations_from_analysis(iden, ancestors)
            }
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => self.find_method_references(receiver, iden, ancestors, namespace_kind, position),
            Identifier::RubyInstanceVariable { name, .. } => {
                if let Ok(fqn) = FullyQualifiedName::instance_variable(name.clone()) {
                    self.find_variable_references(&fqn)
                } else {
                    None
                }
            }
            Identifier::RubyClassVariable { name, .. } => {
                if let Ok(fqn) = FullyQualifiedName::class_variable(name.clone()) {
                    self.find_variable_references(&fqn)
                } else {
                    None
                }
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                if let Ok(fqn) = FullyQualifiedName::global_variable(name.clone()) {
                    self.find_variable_references(&fqn)
                } else {
                    None
                }
            }
            Identifier::RubyLocalVariable { name, .. } => {
                self.find_local_variable_references(name, position)
            }
            Identifier::YardType { type_name, .. } => {
                if let Some(fqn) = YardTypeConverter::parse_type_name_to_fqn_public(type_name) {
                    self.find_constant_references(&fqn)
                } else {
                    None
                }
            }
        }
    }

    fn method_reference_locations_for_namespace_from_analysis(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges(namespace_fqn, method),
        )))
    }

    fn method_reference_locations_for_constant_receiver_from_analysis(
        &self,
        receiver_path: &[RubyConstant],
        ancestors: &[RubyConstant],
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges_for_constant_receiver(receiver_path, ancestors, method),
        )))
    }

    fn method_reference_locations_for_current_scope_from_analysis(
        &self,
        ancestors: &[RubyConstant],
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges_for_current_scope(ancestors, method),
        )))
    }

    fn constant_reference_locations_from_analysis(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.constant_reference_ranges(constant_path, ancestors),
        ))
    }

    fn variable_reference_locations_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.variable_reference_ranges(fqn),
        ))
    }

    fn reference_locations_for_fqn_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.reference_ranges_for_fqn(fqn),
        ))
    }
}
