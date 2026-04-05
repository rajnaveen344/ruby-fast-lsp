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
use tower_lsp::lsp_types::{Location, Position, Url};

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
