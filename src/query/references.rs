//! Reference Query - Find usages of symbols
//!
//! Consolidates reference logic from `capabilities/references.rs`.

use crate::analyzer_prism::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use crate::indexer::entry::{EntryKind, NamespaceKind};
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::scope::LVScopeId;
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
        let (identifier_opt, _, ancestors, _scope_stack) = analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        self.find_references_for_identifier(&identifier, &ancestors)
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

    /// Find references to a method (mixin-aware).
    fn find_method_references(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let mut all_references = Vec::new();

        match receiver {
            MethodReceiver::Constant(receiver_ns) => {
                let receiver_fqn = self.resolve_receiver_fqn(receiver_ns, ancestors);
                if let Some(refs) = self.find_method_refs_with_receiver(&receiver_fqn, method) {
                    all_references.extend(refs);
                }
            }
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                let context_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
                if let Some(refs) = self.find_method_refs_without_receiver(&context_fqn, method) {
                    all_references.extend(refs);
                }
            }
            _ => {
                // Variable/expression receiver - search by method name
                if let Some(refs) = self.find_method_refs_by_name(method) {
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

    /// Find references to a local variable using document.lvars (file-local storage).
    fn find_local_variable_references(
        &self,
        name: &str,
        scope_id: LVScopeId,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();
        let mut all_locations = Vec::new();

        // Get the definition location for this scope
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::LocalVariable(data) = &entry.kind {
                    if &data.name == name {
                        all_locations.push(Location {
                            uri: document.uri.clone(),
                            range: entry.location.range,
                        });
                        break;
                    }
                }
            }
        }

        // Get all references to this local variable (scoped)
        let refs = document.get_lvar_references(name, &[scope_id]);
        all_locations.extend(refs);

        if all_locations.is_empty() {
            None
        } else {
            Some(all_locations)
        }
    }
}

// Private helpers
impl IndexQuery {
    /// Find references for a given identifier.
    fn find_references_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
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
            } => self.find_method_references(receiver, iden, ancestors),
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
            Identifier::RubyLocalVariable { name, scope, .. } => {
                self.find_local_variable_references(name, *scope)
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

    /// Find method references with a receiver.
    fn find_method_refs_with_receiver(
        &self,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let mut all_references = Vec::new();
        // When searching with a constant receiver (e.g., Foo.bar), use singleton namespace
        let kind = NamespaceKind::Singleton;

        if let Some(refs) = self.find_method_refs_in_ancestor_chain(receiver_fqn, method, kind) {
            all_references.extend(refs);
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references without a receiver.
    fn find_method_refs_without_receiver(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let mut all_references = Vec::new();
        // Try instance methods first for bare method calls
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
        // Create Namespace FQN with kind for correct ancestor chain lookup
        let context_ns = FullyQualifiedName::namespace_with_kind(context_fqn.namespace_parts(), kind);
        let ancestor_chain = index.get_ancestor_chain(&context_ns);

        for ancestor_fqn in ancestor_chain {
            let method_fqn =
                FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());
            let refs = index.references(&method_fqn);
            if !refs.is_empty() {
                all_references.extend(refs);
            }

            // Also check including classes
            let including_classes = index.get_including_classes(&ancestor_fqn);
            for including_class_fqn in including_classes {
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
        let including_classes = index.get_including_classes(module_fqn);

        for including_class_fqn in including_classes {
            // Create Namespace FQN with kind for correct ancestor chain lookup
            let class_ns = FullyQualifiedName::namespace_with_kind(including_class_fqn.namespace_parts(), kind);
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

    /// Find method references by name (fallback for expression receivers).
    fn find_method_refs_by_name(&self, method: &RubyMethod) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();

        if let Some(entries) = index.get_methods_by_name(method) {
            for entry in entries {
                if let EntryKind::Method(_) = &entry.kind {
                    if let Some(fqn) = index.get_fqn(entry.fqn_id) {
                        let refs = index.references(fqn);
                        if !refs.is_empty() {
                            all_references.extend(refs);
                        }
                    }
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
