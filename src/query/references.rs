//! Reference Query - Find usages of symbols
//!
//! Consolidates reference logic from `capabilities/references.rs`.

use crate::analyzer_prism::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use crate::indexer::entry::NamespaceKind;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::yard::YardTypeConverter;
use log::info;
use tower_lsp::lsp_types::{Location, Position, Url};

use super::IndexQuery;

impl IndexQuery {
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
        let index = self.index.lock();
        let entries = index.references(fqn);
        if !entries.is_empty() {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a variable (instance, class, or global).
    fn find_variable_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let entries = index.references(fqn);
        if !entries.is_empty() {
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
        if method.get_name() == "initialize" {
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
        let scope_id = document
            .variable_scopes()
            .find_scope_for_variable_at(name, position)?;

        // Use VariableScopes to find all references
        let targets = document
            .variable_scopes()
            .find_rename_targets(name, scope_id);

        if targets.is_empty() {
            return None;
        }

        let mut all_locations = Vec::new();
        for target in targets {
            all_locations.push(target.location);
        }

        Some(all_locations)
    }
}

// Private helpers
impl IndexQuery {
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
        let mut all_references = Vec::new();
        let kind = NamespaceKind::Instance;

        if let Some(refs) = self.find_method_refs_in_ancestor_chain(namespace_fqn, method, kind) {
            all_references.extend(refs);
        }

        if let Some(refs) = self.find_method_refs_in_descendants(namespace_fqn, method, kind) {
            all_references.extend(refs);
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references with a constant receiver (singleton namespace).
    fn find_method_refs_with_receiver(
        &self,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        self.find_method_refs_in_ancestor_chain(receiver_fqn, method, NamespaceKind::Singleton)
    }

    /// Find method references without a receiver (instance method in current scope).
    fn find_method_refs_without_receiver(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let mut all_references = Vec::new();
        let method_kind = NamespaceKind::Instance;

        if let Some(refs) =
            self.find_method_refs_in_ancestor_chain(context_fqn, method, method_kind)
        {
            all_references.extend(refs);
        }

        // Also search in classes that include this module
        if let Some(refs) =
            self.find_method_refs_in_sibling_modules(context_fqn, method, method_kind)
        {
            all_references.extend(refs);
        }

        // Search descendants (subclasses) — a call to `parent_method` in Child < Parent
        // is indexed as Child#parent_method, so we need to check subclasses too
        if let Some(refs) = self.find_method_refs_in_descendants(context_fqn, method, method_kind) {
            all_references.extend(refs);
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references in ancestor chain.
    fn find_method_refs_in_ancestor_chain(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: NamespaceKind,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();
        let context_ns =
            FullyQualifiedName::namespace_with_kind(context_fqn.namespace_parts(), kind);
        let ancestor_chain = index.get_ancestor_chain(&context_ns);

        for ancestor_fqn in ancestor_chain {
            let method_fqn =
                FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());
            let refs = index.references(&method_fqn);
            if !refs.is_empty() {
                all_references.extend(refs);
            }

            // Also check including classes
            let including_classes = index.including_classes(&ancestor_fqn);
            for (including_class_fqn, _via_modules) in including_classes {
                let inc_method_fqn = FullyQualifiedName::method(
                    including_class_fqn.namespace_parts(),
                    method.clone(),
                );
                let inc_refs = index.references(&inc_method_fqn);
                if !inc_refs.is_empty() {
                    all_references.extend(inc_refs);
                }
            }
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references in sibling modules.
    fn find_method_refs_in_sibling_modules(
        &self,
        module_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: NamespaceKind,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();
        let including_classes = index.including_classes(module_fqn);

        for (including_class_fqn, _via_modules) in including_classes {
            let class_ns = FullyQualifiedName::namespace_with_kind(
                including_class_fqn.namespace_parts(),
                kind,
            );
            let ancestor_chain = index.get_ancestor_chain(&class_ns);
            for ancestor_fqn in ancestor_chain {
                let method_fqn =
                    FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());
                let refs = index.references(&method_fqn);
                if !refs.is_empty() {
                    all_references.extend(refs);
                }
            }
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references in descendant classes (subclasses, sub-subclasses, etc.)
    /// and descendants of classes that include this module.
    ///
    /// When `parent_method` is defined in Parent and called as a bare method in
    /// `Child < Parent`, the reference is indexed as `Child#parent_method`.
    /// Similarly, when a module method is called in a subclass of the including class.
    fn find_method_refs_in_descendants(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: NamespaceKind,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();

        let context_ns =
            FullyQualifiedName::namespace_with_kind(context_fqn.namespace_parts(), kind);

        // Collect all FQNs to check descendants of:
        // 1. The context itself (direct subclasses)
        // 2. Classes that include this module (their subclasses too)
        let mut roots_to_search = vec![context_ns.clone()];
        let including_classes = index.including_classes(&context_ns);
        for (class_fqn, _via) in &including_classes {
            roots_to_search.push(FullyQualifiedName::namespace_with_kind(
                class_fqn.namespace_parts(),
                kind,
            ));
        }

        for root in &roots_to_search {
            let descendants = index.descendants(root);
            for descendant_fqn in descendants {
                let method_fqn = FullyQualifiedName::method(
                    descendant_fqn.namespace_parts(),
                    method.clone(),
                );
                let refs = index.references(&method_fqn);
                if !refs.is_empty() {
                    all_references.extend(refs);
                }
            }
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }
}
