//! Method Query - Method resolution helpers
//!
//! Dead simple flow: receiver → namespace → ancestor chain → search
//! All receivers follow the same path through 3 focused search functions.

mod type_inference;

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::analyzer_prism::MethodReceiver;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::NamespaceKind;
use crate::indexer::index::RubyIndex;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use log::{debug, trace};
use tower_lsp::lsp_types::{Location, Position, Url};
use type_inference::TypeInferrer;

use super::IndexQuery;

/// Information about a resolved method.
#[derive(Debug, Clone)]
pub struct MethodInfo {
    /// The fully qualified name of the method.
    pub fqn: FullyQualifiedName,
    /// The return type if known.
    pub return_type: Option<RubyType>,
    /// Whether this is a class method.
    pub is_class_method: bool,
    /// YARD documentation if available.
    pub documentation: Option<String>,
}

impl IndexQuery {
    /// Find definitions for a Ruby method with type-aware filtering.
    ///
    /// Takes the method call AST node data directly from Identifier::RubyMethod
    pub(super) fn find_method_definitions(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: Option<NamespaceKind>,
        position: Position,
    ) -> Option<Vec<Location>> {
        // Get uri and content from IndexQuery context
        let uri = self.uri.as_ref();
        let content = if let Some(doc_arc) = &self.doc {
            doc_arc.read().content.clone()
        } else {
            String::new()
        };
        match receiver {
            // Category 1: Constant receiver - direct namespace resolution
            MethodReceiver::Constant(path) => {
                self.search_constant_receiver(path, method, namespace)
            }

            // Category 2: No receiver - search current scope
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                self.search_current_scope(method, namespace, namespace_kind)
            }

            // Category 3: Variable receiver - try type inference
            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                self.search_variable_receiver(name, method, position, &content)
            }

            // Category 4: Method call receiver - resolve chain type
            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            } => {
                let uri_ref = uri?;
                self.search_method_call_receiver(
                    inner_receiver,
                    method_name,
                    method,
                    uri_ref,
                    position,
                    &content,
                )
            }

            // Category 5: Expression - fallback
            MethodReceiver::Expression => self.search_by_name(method),
        }
    }
}

// ============================================================================
// Receiver Handlers (dispatch to core search functions)
// ============================================================================

impl IndexQuery {
    /// Search for method in constant receiver (e.g., Foo.bar)
    fn search_constant_receiver(
        &self,
        path: &[RubyConstant],
        method: &RubyMethod,
        namespace: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        // 1. Resolve constant to FQN
        let index = self.index.lock();
        let current_fqn = FullyQualifiedName::namespace(namespace.to_vec());
        let receiver_fqn = resolve_constant_fqn_from_parts(&index, path, false, &current_fqn)
            .unwrap_or_else(|| {
                // Fallback to using the path directly
                FullyQualifiedName::Constant(path.to_vec())
            });
        drop(index);

        // 2. Build singleton namespace (Foo.bar means #<Class:Foo>)
        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            receiver_fqn.namespace_parts(),
            NamespaceKind::Singleton,
        );

        // 3. Search - same path as everything else!
        self.search_in_ancestor_chain(&namespace_fqn, method)
    }

    /// Search for method in current scope (no receiver)
    fn search_current_scope(
        &self,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: Option<NamespaceKind>,
    ) -> Option<Vec<Location>> {
        // Use the namespace kind from AST context if available
        if let Some(kind) = namespace_kind {
            let namespace_fqn = FullyQualifiedName::namespace_with_kind(namespace.to_vec(), kind);
            return self.search_in_ancestor_chain(&namespace_fqn, method);
        }

        // Fallback: try Instance first (most common: bare method calls)
        let namespace_instance =
            FullyQualifiedName::namespace_with_kind(namespace.to_vec(), NamespaceKind::Instance);
        if let Some(locations) = self.search_in_ancestor_chain(&namespace_instance, method) {
            return Some(locations);
        }

        // Fallback to Singleton (class method calls in class body)
        let namespace_singleton =
            FullyQualifiedName::namespace_with_kind(namespace.to_vec(), NamespaceKind::Singleton);
        self.search_in_ancestor_chain(&namespace_singleton, method)
    }

    /// Search for method via variable receiver (e.g., x.bar where x is a variable)
    fn search_variable_receiver(
        &self,
        var_name: &str,
        method: &RubyMethod,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        // Try type inference
        let inferrer = TypeInferrer {
            index: &self.index,
            doc: self.doc.as_ref(),
        };

        if let Some(receiver_type) = inferrer.infer_variable_type(var_name, position, content) {
            trace!(
                "Found receiver type for '{}': {:?}",
                var_name,
                receiver_type
            );
            return self.search_with_type(&receiver_type, method);
        }

        // Fallback - search by name only
        self.search_by_name(method)
    }

    /// Search for method via method call receiver (e.g., a.b.c where we need a.b's type)
    fn search_method_call_receiver(
        &self,
        inner_receiver: &MethodReceiver,
        method_name: &str,
        method: &RubyMethod,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        // Try type inference for the chain
        let inferrer = TypeInferrer {
            index: &self.index,
            doc: self.doc.as_ref(),
        };

        if let Some(receiver_type) =
            inferrer.resolve_method_chain_type(inner_receiver, method_name, uri, position, content)
        {
            trace!(
                "Found method call receiver type for '{}.{}': {:?}",
                receiver_to_string(inner_receiver),
                method_name,
                receiver_type
            );
            return self.search_with_type(&receiver_type, method);
        }

        // Fallback - search by name only
        self.search_by_name(method)
    }

    /// Search with type information (used after type inference)
    fn search_with_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        // Extract namespace and kind from type
        let (namespace_parts, kind) = match receiver_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                (fqn.namespace_parts(), NamespaceKind::Instance)
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                (fqn.namespace_parts(), NamespaceKind::Singleton)
            }
            RubyType::Array(_) => (
                vec![RubyConstant::new("Array").ok()?],
                NamespaceKind::Instance,
            ),
            RubyType::Hash(_, _) => (
                vec![RubyConstant::new("Hash").ok()?],
                NamespaceKind::Instance,
            ),
            RubyType::Union(_) | RubyType::Unknown => {
                // For complex types, fall back to name-based search
                return self.search_by_name(method);
            }
        };

        let namespace = FullyQualifiedName::namespace_with_kind(namespace_parts, kind);
        self.search_in_ancestor_chain(&namespace, method)
    }
}

// ============================================================================
// Core Search - 3 Functions with Clear Semantics
// ============================================================================

impl IndexQuery {
    /// Core search - single entry point for ALL receiver types.
    ///
    /// Delegates to either:
    /// - `iterate_chain()` for early return (override semantics)
    /// - `search_all_includers()` for collecting all definitions (module case)
    fn search_in_ancestor_chain(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let ancestor_chain = index.get_ancestor_chain(namespace_fqn);

        // Search own ancestor chain first (early return = override semantics)
        if let Some(locations) = Self::iterate_chain(&index, &ancestor_chain, method) {
            return Some(locations); // Stop at first match
        }

        // Module edge case: collect ALL definitions from includers
        // (e.g., calling `services` from within module M shows all overrides in A, B, C)
        if Self::is_module(&index, namespace_fqn) {
            return Self::search_all_includers(&index, namespace_fqn, method);
        }

        None
    }

    /// Iterate ancestor chain - early return on first match (Ruby's override semantics).
    ///
    /// Example: `Foo.new.bar` finds `Foo#bar`, stops (doesn't check parent `Bar#bar`)
    fn iterate_chain(
        index: &RubyIndex,
        ancestor_chain: &[FullyQualifiedName],
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        for ancestor in ancestor_chain {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), method.clone());

            if let Some(entries) = index.get(&method_fqn) {
                let locations: Vec<_> = entries
                    .iter()
                    .filter(|e| Self::matches_ancestor(e, ancestor_chain))
                    .filter_map(|e| index.to_lsp_location(&e.location))
                    .collect();

                if !locations.is_empty() {
                    debug!(
                        "[Method Found] {}#{} found in ancestor: {}",
                        ancestor_chain.first().unwrap_or(ancestor),
                        method,
                        ancestor
                    );
                    return Some(locations); // Early return - first match wins
                }
            }
        }
        None
    }

    /// Search in ALL classes that include this module - collect ALL definitions.
    ///
    /// Example: Inside `module M`, calling `services` shows all overrides in ClassA, ClassB, etc.
    /// This is different from `iterate_chain` - we collect from ALL includers, not just the first.
    fn search_all_includers(
        index: &RubyIndex,
        module_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let module_id = index.get_fqn_id(module_fqn)?;

        // Get includers from graph (with fallback to scanning)
        let mut includers = index.get_graph().mixers(module_id);
        if includers.is_empty() {
            includers = index
                .get_transitive_mixin_classes(module_fqn)
                .into_iter()
                .filter_map(|fqn| index.get_fqn_id(&fqn))
                .collect();
        }

        // Collect ALL definitions from all includers
        let mut all_locations = Vec::new();
        for includer_id in includers {
            let Some(includer_fqn) = index.get_fqn(includer_id) else {
                continue;
            };
            let includer_chain = index.get_ancestor_chain(includer_fqn);

            // Search this includer's chain (early return within each includer)
            if let Some(locations) = Self::iterate_chain(index, &includer_chain, method) {
                all_locations.extend(locations); // Collect from ALL includers
            }
        }

        if all_locations.is_empty() {
            None
        } else {
            Some(deduplicate_locations(all_locations))
        }
    }
}

// ============================================================================
// Tiny Helpers
// ============================================================================

impl IndexQuery {
    /// Check if entry's owner is in the ancestor chain.
    ///
    /// This ensures we only show methods that are actually callable from this context.
    fn matches_ancestor(
        entry: &crate::indexer::entry::Entry,
        chain: &[FullyQualifiedName],
    ) -> bool {
        let EntryKind::Method(data) = &entry.kind else {
            return true;
        };

        chain.iter().any(|ancestor| {
            ancestor.namespace_parts() == data.owner.namespace_parts()
                && ancestor.namespace_kind() == data.owner.namespace_kind()
        })
    }

    /// Check if FQN represents a module (not a class).
    fn is_module(index: &RubyIndex, fqn: &FullyQualifiedName) -> bool {
        if let Some(entries) = index.get(fqn) {
            if let Some(entry) = entries.first() {
                return matches!(entry.kind, EntryKind::Module(_));
            }
        }
        false
    }

    /// Fallback search by method name only (no type filtering).
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

// ============================================================================
// Utils
// ============================================================================

fn receiver_to_string(receiver: &MethodReceiver) -> String {
    match receiver {
        MethodReceiver::None => "".to_string(),
        MethodReceiver::SelfReceiver => "self".to_string(),
        MethodReceiver::Constant(path) => path
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        MethodReceiver::LocalVariable(name)
        | MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => name.clone(),
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => format!("{}.{}", receiver_to_string(inner_receiver), method_name),
        MethodReceiver::Expression => "<expr>".to_string(),
    }
}

fn deduplicate_locations(locations: Vec<Location>) -> Vec<Location> {
    let mut unique_locations = Vec::new();
    for location in locations {
        if !unique_locations.iter().any(|existing: &Location| {
            existing.uri == location.uri && existing.range == location.range
        }) {
            unique_locations.push(location);
        }
    }
    unique_locations
}
