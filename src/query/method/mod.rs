//! Method Query - Method definition resolution
//!
//! ## Flow
//!
//! ```text
//! find_method_definitions()
//!   ↓
//! 1. Resolve receiver → namespace FQN
//! 2. Determine FQNs to search (class: [self], module: [all includers])
//! 3. Search each FQN's ancestors
//! 4. Collect and return all definitions
//! ```

mod helpers;
mod type_inference;

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::analyzer_prism::MethodReceiver;
use crate::indexer::entry::NamespaceKind;
use crate::indexer::index::RubyIndex;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::utils::deduplicate_locations;
use helpers::{
    get_module_includers, is_module_instance_namespace, matches_ancestor, receiver_to_string,
};
use log::{debug, trace};
use tower_lsp::lsp_types::{Location, Position};
use type_inference::TypeInferrer;

use super::IndexQuery;

// ============================================================================
// Public API
// ============================================================================

/// Information about a resolved method.
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub fqn: FullyQualifiedName,
    pub return_type: Option<RubyType>,
    pub is_class_method: bool,
    pub documentation: Option<String>,
}

impl IndexQuery {
    /// Find definitions for a Ruby method call.
    ///
    /// Algorithm:
    /// 1. Resolve receiver → namespace FQN
    /// 2. Determine FQNs to search:
    ///    - Class: [class_fqn]
    ///    - Module instance: [all includer FQNs]
    /// 3. Search each FQN's ancestor chain
    /// 4. Collect all definitions
    pub fn find_method_definitions(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        // 1. Resolve receiver → namespace FQN
        let namespace_fqn =
            match self.resolve_receiver_to_namespace(receiver, namespace, namespace_kind, position)
            {
                Some(fqn) => fqn,
                None => {
                    // Type inference failed - fall back to name-only search
                    return self.search_by_name(method);
                }
            };

        let index = self.index.lock();

        // 2. Determine which FQNs to search
        let fqns_to_search = if is_module_instance_namespace(&index, &namespace_fqn) {
            // Module: search all includers' chains
            // WHY: When module M is included in ClassA and ClassB, calling a method from M
            // should find both ClassA#method and ClassB#method (different overrides).
            let includers = get_module_includers(&index, &namespace_fqn);
            if includers.is_empty() {
                // No includers? Search module's own chain
                vec![namespace_fqn]
            } else {
                includers
            }
        } else {
            // Class or module singleton: search just this namespace
            vec![namespace_fqn]
        };

        // 3. Search each FQN's ancestor chain
        let mut all_definitions = Vec::new();
        for fqn in fqns_to_search {
            let ancestor_chain = index.get_ancestor_chain(&fqn);
            if let Some(definitions) = self.search_ancestor_chain(&index, &ancestor_chain, method) {
                all_definitions.extend(definitions);
            }
        }

        drop(index);

        // 4. Return results (with fallback)
        if all_definitions.is_empty() {
            self.search_by_name(method) // Fallback: search by name only
        } else {
            Some(deduplicate_locations(all_definitions))
        }
    }
}

// ============================================================================
// Receiver Resolution (Receiver → Namespace FQN)
// ============================================================================

impl IndexQuery {
    /// Convert method receiver to namespace FQN.
    fn resolve_receiver_to_namespace(
        &self,
        receiver: &MethodReceiver,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        match receiver {
            MethodReceiver::Constant(path) => {
                self.resolve_constant_receiver(path, current_namespace)
            }

            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                self.resolve_current_scope(current_namespace, namespace_kind)
            }

            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                self.resolve_variable_receiver(name, position)
            }

            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            } => self.resolve_method_call_receiver(inner_receiver, method_name, position),

            MethodReceiver::Expression => None, // No type info available
        }
    }

    /// Resolve constant receiver: `Foo.bar` → `Namespace(["Foo"], Singleton)`
    fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let index = self.index.lock();
        let current_fqn = FullyQualifiedName::namespace(current_namespace.to_vec());

        let receiver_fqn = resolve_constant_fqn_from_parts(&index, path, false, &current_fqn)
            .unwrap_or_else(|| FullyQualifiedName::Constant(path.to_vec()));
        drop(index);

        // Constant receiver means calling a class/module method (singleton)
        Some(FullyQualifiedName::namespace_with_kind(
            receiver_fqn.namespace_parts(),
            NamespaceKind::Singleton,
        ))
    }

    /// Resolve current scope: `bar` in context of current namespace
    fn resolve_current_scope(
        &self,
        namespace: &[RubyConstant],
        kind: NamespaceKind,
    ) -> Option<FullyQualifiedName> {
        Some(FullyQualifiedName::namespace_with_kind(
            namespace.to_vec(),
            kind,
        ))
    }

    /// Resolve variable receiver: `x.bar` where x is a variable
    fn resolve_variable_receiver(
        &self,
        var_name: &str,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        let content = self.doc.as_ref()?.read().content.clone();
        let inferrer = TypeInferrer {
            index: &self.index,
            doc: self.doc.as_ref(),
        };

        let var_type = inferrer.infer_variable_type(var_name, position, &content)?;
        trace!("Inferred type for '{}': {:?}", var_name, var_type);

        self.convert_type_to_namespace(&var_type)
    }

    /// Resolve method call receiver: `a.b.c` where we need a.b's return type
    fn resolve_method_call_receiver(
        &self,
        inner_receiver: &MethodReceiver,
        method_name: &str,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        let uri = self.uri.as_ref()?;
        let content = self.doc.as_ref()?.read().content.clone();
        let inferrer = TypeInferrer {
            index: &self.index,
            doc: self.doc.as_ref(),
        };

        let chain_type = inferrer.resolve_method_chain_type(
            inner_receiver,
            method_name,
            uri,
            position,
            &content,
        )?;
        trace!(
            "Inferred type for '{}.{}': {:?}",
            receiver_to_string(inner_receiver),
            method_name,
            chain_type
        );

        self.convert_type_to_namespace(&chain_type)
    }

    /// Convert RubyType to namespace FQN
    fn convert_type_to_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    NamespaceKind::Instance,
                ))
            }

            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    NamespaceKind::Singleton,
                ))
            }

            RubyType::Array(_) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Array").ok()?],
                NamespaceKind::Instance,
            )),

            RubyType::Hash(_, _) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Hash").ok()?],
                NamespaceKind::Instance,
            )),

            RubyType::Union(_) | RubyType::Unknown => None,
        }
    }
}

// ============================================================================
// Ancestor Chain Search
// ============================================================================

impl IndexQuery {
    /// Search ancestor chain with early return (Ruby's override semantics).
    ///
    /// Searches [Self, Prepends, Includes, Superclass, ...] and returns first match.
    fn search_ancestor_chain(
        &self,
        index: &RubyIndex,
        ancestor_chain: &[FullyQualifiedName],
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        for ancestor in ancestor_chain {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), method.clone());

            if let Some(entries) = index.get(&method_fqn) {
                let locations: Vec<_> = entries
                    .iter()
                    .filter(|e| matches_ancestor(e, ancestor_chain))
                    .filter_map(|e| index.to_lsp_location(&e.location))
                    .collect();

                if !locations.is_empty() {
                    debug!(
                        "[Method Found] {}#{} in {}",
                        ancestor_chain.first().unwrap_or(ancestor),
                        method,
                        ancestor
                    );
                    return Some(locations); // Early return - override semantics
                }
            }
        }
        None
    }
}

// ============================================================================
// Fallback Search
// ============================================================================

impl IndexQuery {
    /// Fallback: search by method name only (no type filtering).
    fn search_by_name(&self, method: &RubyMethod) -> Option<Vec<Location>> {
        let index = self.index.lock();
        index.get_methods_by_name(method).and_then(|entries| {
            let locations: Vec<Location> = entries
                .iter()
                .filter_map(|entry| index.to_lsp_location(&entry.location))
                .collect();

            if locations.is_empty() {
                None
            } else {
                Some(locations)
            }
        })
    }
}