//! Implementation Query - Find where methods/modules are concretely implemented
//!
//! Answers "textDocument/implementation":
//! - For a method: find all overrides in descendant classes and including classes
//! - For a module/class: find all classes that include/prepend/extend it

use std::collections::HashSet;

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use log::info;
use ruby_analysis_core::GraphEdgeKind;
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::location_for_range;
use super::IndexQuery;

impl IndexQuery {
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
        if let Some(locations) = self.method_implementations_from_analysis(owner_fqn, method) {
            return Some(locations);
        }

        let index = self.index.lock();

        let namespaces_to_check = collect_all_implementors(&index, owner_fqn);

        let mut locations = Vec::new();
        for ns_fqn in &namespaces_to_check {
            let method_fqn =
                FullyQualifiedName::method(ns_fqn.namespace_parts().to_vec(), method.clone());

            if let Some(entries) = index.get(&method_fqn) {
                for entry in entries {
                    if matches!(entry.kind, EntryKind::Method(_)) {
                        if let Some(loc) = index.to_lsp_location(&entry.location) {
                            locations.push(loc);
                        }
                    }
                }
            }
        }

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Find all implementations of a module/class.
    ///
    /// For a module: returns all classes/modules that include/prepend/extend it
    /// (transitively through module chains), plus all subclasses.
    /// For a class: returns all subclasses.
    fn find_namespace_implementations(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(locations) = self.namespace_implementations_from_analysis(fqn) {
            return Some(locations);
        }

        let index = self.index.lock();

        let implementors = collect_all_implementors(&index, fqn);

        let mut locations = Vec::new();
        for impl_fqn in &implementors {
            if let Some(entries) = index.get(impl_fqn) {
                for entry in entries {
                    if matches!(entry.kind, EntryKind::Class(_) | EntryKind::Module(_)) {
                        if let Some(loc) = index.to_lsp_location(&entry.location) {
                            locations.push(loc);
                        }
                    }
                }
            }
        }

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    fn method_implementations_from_analysis(
        &self,
        owner_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let namespaces_to_check = collect_all_implementors_from_analysis(&engine, owner_fqn);
        if namespaces_to_check.is_empty() {
            return None;
        }

        let mut locations = Vec::new();
        for ns_fqn in &namespaces_to_check {
            let method_fqn = FullyQualifiedName::method(ns_fqn.namespace_parts(), *method);
            for fact in engine.method_facts_for(&method_fqn) {
                if fact.owner.namespace_parts() == ns_fqn.namespace_parts()
                    && fact.owner.namespace_kind() == ns_fqn.namespace_kind()
                {
                    if let Some(location) = location_for_range(&engine, fact.range) {
                        locations.push(location);
                    }
                }
            }
        }

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
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let implementors = collect_all_implementors_from_analysis(&engine, fqn);
        if implementors.is_empty() {
            return None;
        }

        let locations = implementors
            .iter()
            .filter_map(|impl_fqn| engine.graph_nodes_for(impl_fqn).first())
            .filter_map(|fact| location_for_range(&engine, fact.range))
            .collect::<Vec<_>>();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }
}

/// Collect all namespaces that could implement/override something from `origin_fqn`.
///
/// Performs a BFS walk:
/// 1. `descendants(origin)` — subclasses, sub-subclasses, etc.
/// 2. `mixers(origin)` — direct includers/prependers (both modules and classes)
/// 3. For each mixer: also collect its mixers AND its descendants
///
/// Uses a visited set to avoid cycles (e.g., circular includes) and duplicates.
fn collect_all_implementors(
    index: &RubyIndex,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = vec![origin_fqn.clone()];

    // Mark the origin as visited so we don't include it in results
    visited.insert(origin_fqn.clone());

    while let Some(current) = queue.pop() {
        // 1. Descendants (subclasses, transitively — descendants() is already transitive)
        for desc in index.descendants(&current) {
            if visited.insert(desc.clone()) {
                result.push(desc);
            }
        }

        // 2. Mixers (include/prepend, direct only — we walk transitively via the queue)
        for mixer in index.mixers(&current) {
            if visited.insert(mixer.clone()) {
                result.push(mixer.clone());
                queue.push(mixer);
            }
        }
    }

    result
}

fn collect_all_implementors_from_analysis(
    engine: &ruby_analysis_engine::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = vec![origin_fqn.clone()];

    visited.insert(origin_fqn.clone());

    while let Some(current) = queue.pop() {
        for descendant in descendants_from_analysis(engine, &current) {
            if visited.insert(descendant.clone()) {
                result.push(descendant);
            }
        }

        for mixer in mixers_from_analysis(engine, &current) {
            if visited.insert(mixer.clone()) {
                result.push(mixer.clone());
                queue.push(mixer);
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn descendants_from_analysis(
    engine: &ruby_analysis_engine::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    collect_descendants_from_analysis(engine, origin_fqn, &mut result, &mut visited);
    result
}

fn collect_descendants_from_analysis(
    engine: &ruby_analysis_engine::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
    result: &mut Vec<FullyQualifiedName>,
    visited: &mut HashSet<FullyQualifiedName>,
) {
    if !visited.insert(origin_fqn.clone()) {
        return;
    }

    for edge in engine.all_graph_edges() {
        if edge.kind == GraphEdgeKind::Superclass && edge.target == *origin_fqn {
            result.push(edge.source.clone());
            collect_descendants_from_analysis(engine, &edge.source, result, visited);
        }
    }
}

fn mixers_from_analysis(
    engine: &ruby_analysis_engine::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut mixers = engine
        .all_graph_edges()
        .into_iter()
        .filter(|edge| {
            edge.target == *origin_fqn
                && matches!(
                    edge.kind,
                    GraphEdgeKind::Include | GraphEdgeKind::Prepend | GraphEdgeKind::Extend
                )
        })
        .map(|edge| edge.source)
        .collect::<Vec<_>>();
    mixers.sort_by_key(|fqn| fqn.to_string());
    mixers.dedup();
    mixers
}
