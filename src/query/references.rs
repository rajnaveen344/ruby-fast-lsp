//! Reference Query - Find usages of symbols
//!
//! Consolidates reference logic from `capabilities/references.rs`.

use log::info;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::yard::YardTypeConverter;
use ruby_analysis::indexer::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::location_for_range;
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
        if let Some(entries) = self.reference_locations_from_analysis(fqn) {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a variable (instance, class, or global).
    fn find_variable_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.reference_locations_from_analysis(fqn) {
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
                let context_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
                return self.find_method_refs_with_receiver(&context_fqn, &new_method);
            }
        }

        match receiver {
            MethodReceiver::Constant(receiver_ns) => {
                let receiver_fqn = self.resolve_receiver_fqn(receiver_ns, ancestors);
                self.find_method_refs_with_receiver(&receiver_fqn, method)
            }
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                let context_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
                self.find_method_refs_without_receiver(&context_fqn, method)
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
                self.find_method_refs_for_resolved_namespace(&resolved_ns, method)
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

        // Use position-based lookup to find the scope owning this variable
        let scope_id = document.find_scope_for_variable_at(name, position)?;

        // Use VariableScopes to find all references
        let targets = document
            .variable_scopes()
            .find_rename_targets(name, scope_id);

        if targets.is_empty() {
            return None;
        }

        let mut all_locations = Vec::new();
        for target in targets {
            all_locations.push(document.text_range_to_lsp_location(target.location));
        }

        Some(all_locations)
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
                let mut combined_ns = ancestors.to_vec();
                combined_ns.extend(iden.clone());

                // Try as Namespace first (for class/module references)
                let namespace_fqn = FullyQualifiedName::namespace(combined_ns.clone());
                if let Some(refs) = self.find_constant_references(&namespace_fqn) {
                    return Some(refs);
                }

                // Then try as Constant (for value constant references like VALUE = 42)
                let constant_fqn = FullyQualifiedName::Constant(combined_ns);
                self.find_constant_references(&constant_fqn)
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

    /// Resolve receiver FQN from namespace path.
    fn resolve_receiver_fqn(
        &self,
        receiver_ns: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> FullyQualifiedName {
        if !receiver_ns.is_empty() && !ancestors.is_empty() {
            let first_receiver_part = &receiver_ns[0];
            if let Some(pos) = ancestors.iter().position(|c| c == first_receiver_part) {
                let mut resolved_ns = ancestors[..=pos].to_vec();
                resolved_ns.extend(receiver_ns[1..].iter().cloned());
                return FullyQualifiedName::Constant(resolved_ns);
            } else {
                let mut full_ns = vec![ancestors[0].clone()];
                full_ns.extend(receiver_ns.iter().cloned());
                return FullyQualifiedName::Constant(full_ns);
            }
        }
        let mut full_ns = ancestors.to_vec();
        full_ns.extend(receiver_ns.iter().cloned());
        FullyQualifiedName::Constant(full_ns)
    }

    /// Find method references when the receiver has been resolved to a namespace FQN.
    /// Searches the namespace's ancestor chain, descendants, and including classes.
    fn find_method_refs_for_resolved_namespace(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        if let Some(locations) = self.find_method_refs_from_analysis(namespace_fqn, method) {
            return Some(locations);
        }
        None
    }

    /// Find method references with a constant receiver (singleton namespace).
    fn find_method_refs_with_receiver(
        &self,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            receiver_fqn.namespace_parts(),
            NamespaceKind::Singleton,
        );
        if let Some(locations) = self.find_method_refs_from_analysis(&namespace_fqn, method) {
            return Some(locations);
        }
        None
    }

    /// Find method references without a receiver (instance method in current scope).
    fn find_method_refs_without_receiver(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            context_fqn.namespace_parts(),
            NamespaceKind::Instance,
        );
        if let Some(locations) = self.find_method_refs_from_analysis(&namespace_fqn, method) {
            return Some(locations);
        }
        None
    }

    fn find_method_refs_from_analysis(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        let locations = query
            .method_reference_ranges(namespace_fqn, method)
            .into_iter()
            .filter_map(|range| location_for_range(&engine, range))
            .collect::<Vec<_>>();
        if locations.is_empty() {
            None
        } else {
            Some(crate::utils::deduplicate_locations(locations))
        }
    }

    fn reference_locations_from_analysis(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = AnalysisQuery::new(&engine);
        let locations = query
            .reference_ranges_for_fqn(fqn)
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
