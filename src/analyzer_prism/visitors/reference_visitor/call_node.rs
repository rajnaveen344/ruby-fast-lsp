use log::trace;
use ruby_prism::{CallNode, Node};
use tower_lsp::lsp_types::Url;

use crate::{
    analyzer_prism::{diagnostics::ReceiverInfo, utils},
    indexer::{entry::NamespaceKind, index::UnresolvedEntry},
    inferrer::{method::resolver::MethodResolver, r#type::ruby::RubyType},
    types::{
        compact_location::CompactLocation, fully_qualified_name::FullyQualifiedName,
        ruby_method::RubyMethod, ruby_namespace::RubyConstant,
    },
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name = utils::utf8_str(node.name().as_slice());

        // Skip method names that don't follow Ruby method naming conventions
        if !RubyMethod::is_valid_ruby_method_name(&method_name) {
            trace!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        // Get the full call location for references
        let call_location = self
            .document
            .prism_location_to_lsp_location(&node.location());

        // Get the message (method name) location for diagnostics - only underline the method name
        let message_location = node
            .message_loc()
            .map(|loc| self.document.prism_location_to_lsp_location(&loc))
            .unwrap_or_else(|| call_location.clone());

        let current_namespace = self.scope_tracker.get_ns_stack();

        // Determine the target namespace, namespace kind, receiver info, and (for
        // expression receivers) the inferred receiver type used for diagnostics.
        let (target_namespace, namespace_kind, receiver_info, inferred_expr_type) =
            match node.receiver() {
                Some(receiver_node) => {
                    self.handle_receiver_node_with_info(&receiver_node, &current_namespace)
                }
                None => {
                    let (ns, kind) = self.handle_no_receiver(&current_namespace);
                    (ns, kind, ReceiverInfo::NoReceiver, None)
                }
            };

        // For expression receivers, skip *reference indexing* if type inference failed.
        // Indexing under the wrong FQN causes false positives in find-references.
        // Unresolved-method tracking still proceeds — we want to flag the call.
        let inference_failed = matches!(
            receiver_info,
            ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath
        ) && target_namespace == current_namespace;

        // Create the method, handling potential validation errors gracefully
        let method = match RubyMethod::new(&method_name) {
            Ok(method) => method,
            Err(err) => {
                trace!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        let method_fqn = FullyQualifiedName::method(target_namespace.clone(), method);

        // Run all reference resolution + diagnostic lookups under a SHARED
        // read lock. Writes are staged into `self.staged` and flushed under
        // a single write lock at end-of-file. This is safe because Phase 2
        // never mutates definitions — only references + unresolved entries,
        // neither of which feed back into the read path.
        let caller_fqn = self.scope_tracker.current_method_fqn().cloned();
        if !inference_failed {
            trace!(
                "Adding method call reference: {} at {:?}",
                method_fqn.to_string(),
                call_location
            );
            self.staged
                .push_reference(method_fqn.clone(), call_location, caller_fqn);
        } else {
            trace!(
                "Skipping reference for unresolved expression receiver: .{}",
                method_name
            );
        }

        // Collect everything we need from the index up-front under a read
        // guard; drop the guard before pushing into `self.staged` so we
        // never hold the read lock while also mutating `self`.
        let diagnostics_to_stage: Vec<(Url, UnresolvedEntry)> = {
            let index = self.index.read();
            let mut out: Vec<(Url, UnresolvedEntry)> = Vec::new();

            if self.track_unresolved {
                if let Some(entry) = crate::analyzer_prism::diagnostics::unresolved_method::check(
                    &receiver_info,
                    inferred_expr_type.as_ref(),
                    &method_name,
                    &target_namespace,
                    namespace_kind,
                    &message_location,
                    &*index,
                ) {
                    trace!("Adding unresolved method call: {}", method_name);
                    out.push((self.document.uri.clone(), entry));
                }
            }

            if self.track_unresolved {
                let entries = crate::analyzer_prism::diagnostics::signature_mismatch::check(
                    node,
                    &receiver_info,
                    inferred_expr_type.as_ref(),
                    &method_name,
                    &target_namespace,
                    namespace_kind,
                    &message_location,
                    &self.document,
                    &*index,
                );
                for entry in entries {
                    out.push((self.document.uri.clone(), entry));
                }
            }

            out
        };

        for (uri, entry) in diagnostics_to_stage {
            self.staged.push_unresolved(uri, entry);
        }

        // Raise-non-exception check: bare `raise` with provably non-Exception
        // arg. The module acquires short-lived read locks internally — no outer
        // index guard is held here so no reentrancy.
        if self.track_unresolved && method_name == "raise" && node.receiver().is_none() {
            if let Some(entry) = crate::analyzer_prism::diagnostics::raise_non_exception::check(
                node,
                &self.index,
                &self.document,
                &self.scope_tracker.get_ns_stack(),
            ) {
                self.staged
                    .push_unresolved(self.document.uri.clone(), entry);
            }
        }

        // Bad-splat check: *expr must be Array-like; **expr must be Hash-like.
        if self.track_unresolved {
            for entry in crate::analyzer_prism::diagnostics::bad_splat::check(node, &self.document)
            {
                self.staged
                    .push_unresolved(self.document.uri.clone(), entry);
            }
        }
    }

    /// Handle method calls without a receiver (e.g., `method_name`)
    fn handle_no_receiver(
        &self,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        // Determine namespace kind based on current method context
        let namespace_kind = self.scope_tracker.current_method_context();

        (current_namespace.clone(), namespace_kind)
    }

    /// Handle method calls with a receiver node. Returns `(namespace, namespace_kind,
    /// receiver_info, inferred_expr_type)` — `inferred_expr_type` is `Some(_)` only
    /// for expression receivers where type inference returned a result.
    fn handle_receiver_node_with_info(
        &self,
        receiver_node: &Node,
        current_namespace: &Vec<RubyConstant>,
    ) -> (
        Vec<RubyConstant>,
        NamespaceKind,
        ReceiverInfo,
        Option<RubyType>,
    ) {
        if receiver_node.as_self_node().is_some() {
            let (ns, kind) = self.handle_self_receiver(current_namespace);
            (ns, kind, ReceiverInfo::SelfReceiver, None)
        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
            let name = utils::utf8_str(constant_read.name().as_slice()).to_string();
            let (ns, kind) = self.handle_constant_read_receiver(&constant_read, current_namespace);
            (ns, kind, ReceiverInfo::ConstantReceiver(name), None)
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            if self.is_valid_constant_path_receiver(receiver_node) {
                let receiver_name =
                    crate::analyzer_prism::utils::build_constant_path_name(receiver_node);
                let (ns, kind) = self.handle_constant_path_receiver(
                    &constant_path,
                    receiver_node,
                    current_namespace,
                );
                (
                    ns,
                    kind,
                    ReceiverInfo::ConstantReceiver(receiver_name),
                    None,
                )
            } else {
                let (ns, kind, inferred) =
                    self.handle_expression_receiver(receiver_node, current_namespace);
                (ns, kind, ReceiverInfo::InvalidConstantPath, inferred)
            }
        } else {
            let (ns, kind, inferred) =
                self.handle_expression_receiver(receiver_node, current_namespace);
            (ns, kind, ReceiverInfo::ExpressionReceiver, inferred)
        }
    }

    /// Check if a constant path receiver is valid (only contains constant paths and constant reads)
    fn is_valid_constant_path_receiver(&self, node: &Node) -> bool {
        if node.as_constant_read_node().is_some() {
            return true;
        }

        if let Some(constant_path) = node.as_constant_path_node() {
            // Check if the parent is valid (if present)
            if let Some(parent) = constant_path.parent() {
                return self.is_valid_constant_path_receiver(&parent);
            }
            // No parent means it's a root constant path (::Foo), which is valid
            return true;
        }

        // Any other node type is invalid
        false
    }

    /// Handle method calls with self receiver (e.g., `self.method_name`)
    fn handle_self_receiver(
        &self,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        // Self receiver - method is in current namespace
        (current_namespace.clone(), NamespaceKind::Instance)
    }

    /// Handle method calls with constant read receiver (e.g., `Class.method`)
    fn handle_constant_read_receiver(
        &self,
        constant_read: &ruby_prism::ConstantReadNode,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        let name = utils::utf8_str(constant_read.name().as_slice());
        if let Ok(constant) = RubyConstant::new(name) {
            let mut receiver_namespace = current_namespace.clone();
            receiver_namespace.push(constant);
            (receiver_namespace, NamespaceKind::Singleton)
        } else {
            // Fallback to instance method if constant parsing fails
            (current_namespace.clone(), NamespaceKind::Instance)
        }
    }

    /// Handle method calls with constant path receiver (e.g., `Module::Class.method`)
    fn handle_constant_path_receiver(
        &self,
        _constant_path: &ruby_prism::ConstantPathNode,
        receiver_node: &Node,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        // Use the centralized constant resolution utility
        let current_fqn = FullyQualifiedName::Constant(current_namespace.clone());
        let index_guard = self.index.lock();
        if let Some(resolved_fqn) =
            utils::resolve_constant_fqn(&index_guard, receiver_node, &current_fqn)
        {
            if let FullyQualifiedName::Constant(parts) = resolved_fqn {
                return (parts, NamespaceKind::Singleton);
            }
        }

        // Fallback to mixin_ref approach if resolution fails
        // Use default location since we're just extracting namespace info, not tracking mixin call sites
        if let Some(mixin_ref) =
            utils::mixin_ref_from_node(receiver_node, CompactLocation::default())
        {
            let final_namespace = if mixin_ref.absolute {
                mixin_ref.parts
            } else {
                self.resolve_relative_constant_path(&mixin_ref.parts, current_namespace)
            };
            (final_namespace, NamespaceKind::Singleton)
        } else {
            // Fallback to instance method if constant path resolution fails
            (current_namespace.clone(), NamespaceKind::Instance)
        }
    }

    /// Handle method calls with expression receiver (e.g., `variable.method`).
    ///
    /// Tries to infer the receiver's type using:
    /// 1. Local variable → constructor pattern matching (`x = Foo.new`)
    /// 2. Method call chain → return type resolution (`a.b` → resolve b's return type)
    ///
    /// Falls back to current namespace if inference fails. The caller checks whether
    /// inference changed the result and skips indexing if it didn't (to avoid false positives).
    fn handle_expression_receiver(
        &self,
        receiver_node: &Node,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, NamespaceKind, Option<RubyType>) {
        let inferred = self.infer_expression_receiver_type(receiver_node);
        if let Some(ref resolved_type) = inferred {
            if let Some(ns) = self.type_to_namespace_parts(resolved_type) {
                return (ns, NamespaceKind::Instance, Some(resolved_type.clone()));
            }
        }

        // Fallback — caller will detect that namespace didn't change and skip indexing
        (current_namespace.clone(), NamespaceKind::Instance, inferred)
    }

    /// Try to infer the type of an expression receiver node.
    fn infer_expression_receiver_type(&self, receiver_node: &Node) -> Option<RubyType> {
        // Case 1: Local variable (e.g., `user.name` where `user = User.new`)
        if let Some(local_var) = receiver_node.as_local_variable_read_node() {
            let var_name = utils::utf8_str(local_var.name().as_slice());
            return self.infer_variable_type_cached(var_name);
        }

        // Case 2: Method call chain (e.g., `team.leader.name`)
        if let Some(call) = receiver_node.as_call_node() {
            let inner_method = utils::utf8_str(call.name().as_slice());

            // First resolve the inner receiver's type
            let inner_type = if let Some(inner_receiver) = call.receiver() {
                if let Some(constant_read) = inner_receiver.as_constant_read_node() {
                    // Constant receiver: Foo.bar → ClassReference(Foo)
                    let name = utils::utf8_str(constant_read.name().as_slice());
                    Some(RubyType::ClassReference(FullyQualifiedName::Constant(
                        vec![RubyConstant::new(name).ok()?],
                    )))
                } else {
                    // Recursive: try to infer inner receiver's type
                    self.infer_expression_receiver_type(&inner_receiver)
                }
            } else {
                // No receiver (bare method call) - type is current class
                let ns = self.scope_tracker.get_ns_stack();
                if ns.is_empty() {
                    None
                } else {
                    Some(RubyType::Class(FullyQualifiedName::Constant(ns)))
                }
            }?;

            // Now resolve the method's return type. Pure read — use the
            // shared guard so multiple workers can recurse in parallel.
            let index = self.index.read();
            return MethodResolver::resolve_method_return_type(&*index, &inner_type, &inner_method);
        }

        None
    }

    /// Convert a RubyType to namespace parts for FQN construction.
    fn type_to_namespace_parts(&self, ruby_type: &RubyType) -> Option<Vec<RubyConstant>> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => Some(fqn.namespace_parts()),
            _ => None,
        }
    }

    /// Resolve relative constant paths by checking namespace hierarchy
    fn resolve_relative_constant_path(
        &self,
        parts: &Vec<RubyConstant>,
        current_namespace: &Vec<RubyConstant>,
    ) -> Vec<RubyConstant> {
        // For relative paths, resolve by checking namespace hierarchy
        // Ruby constant resolution: look for the first part in current namespace and ancestors
        if let Some(first_part) = parts.first() {
            let mut resolved = None;

            // Check if the first constant exists in current namespace or ancestors
            for i in (0..=current_namespace.len()).rev() {
                let test_namespace = &current_namespace[..i];

                // Check if first_part exists at this level
                // Look for the constant in the namespace parts
                if test_namespace
                    .iter()
                    .any(|c| c.to_string() == first_part.to_string())
                {
                    // Found the constant in the namespace hierarchy
                    // Find the position where this constant appears
                    if let Some(pos) = test_namespace
                        .iter()
                        .position(|c| c.to_string() == first_part.to_string())
                    {
                        let mut result = test_namespace[..=pos].to_vec();
                        // Skip the first part since it's already in the namespace
                        result.extend(parts.iter().skip(1).cloned());

                        resolved = Some(result);
                        break;
                    }
                }
            }

            // If not found in hierarchy, try from root
            resolved.unwrap_or_else(|| {
                // Check if it should be resolved from a parent namespace
                // For Platform::PlatformServices in GoshPosh::Platform::SpecHelpers,
                // Platform should resolve to GoshPosh::Platform
                if current_namespace.len() >= 2 {
                    let parent_ns = &current_namespace[..current_namespace.len() - 1];
                    if parent_ns.last().map(|c| c.to_string()) == Some(first_part.to_string()) {
                        let mut result = parent_ns.to_vec();
                        result.extend(parts.iter().cloned());
                        return result;
                    }
                }

                // Default: append to current namespace
                let mut ns = current_namespace.clone();
                ns.extend(parts.iter().cloned());
                ns
            })
        } else {
            current_namespace.clone()
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // Nothing to do on exit
    }
}
