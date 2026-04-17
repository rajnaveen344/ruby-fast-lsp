use log::trace;
use ruby_prism::{CallNode, Node};

use crate::{
    analyzer_prism::utils,
    indexer::{
        entry::{
            entry_kind::{EntryKind, ParamKind},
            NamespaceKind,
        },
        index::UnresolvedEntry,
    },
    inferrer::{method::resolver::MethodResolver, r#type::ruby::RubyType},
    types::{
        compact_location::CompactLocation, fully_qualified_name::FullyQualifiedName,
        ruby_method::RubyMethod, ruby_namespace::RubyConstant,
    },
};

use super::ReferenceVisitor;

/// Positional arity model derived from a method's parameter list.
/// Keyword/keyword-rest/block params don't affect positional arity.
#[derive(Debug, Clone, Copy)]
struct MethodArity {
    required: usize,
    optional: usize,
    has_rest: bool,
}

impl MethodArity {
    fn from_params(params: &[crate::indexer::entry::entry_kind::MethodParamInfo]) -> Self {
        let mut required = 0usize;
        let mut optional = 0usize;
        let mut has_rest = false;
        for p in params {
            match p.kind {
                ParamKind::Required => required += 1,
                ParamKind::Optional => optional += 1,
                ParamKind::Rest => has_rest = true,
                // Kwargs/block don't constrain positional arity in V1.
                ParamKind::Keyword | ParamKind::KeywordRest | ParamKind::Block => {}
            }
        }
        Self {
            required,
            optional,
            has_rest,
        }
    }
}

/// Returns `Some((min, max, actual))` if callsite positional arity is outside
/// `[min, max]`. `max` is `None` when the method accepts `*args`. Returns `None`
/// when arity matches OR when the callsite contains a splat (unknown count).
fn compute_arity_mismatch(
    node: &CallNode,
    arity: &MethodArity,
) -> Option<(usize, Option<usize>, usize)> {
    let mut positional = 0usize;
    let mut has_splat_at_callsite = false;
    if let Some(args) = node.arguments() {
        for arg in args.arguments().iter() {
            if arg.as_splat_node().is_some() {
                has_splat_at_callsite = true;
                continue;
            }
            // Skip keyword hash and block-arg from positional count.
            if arg.as_keyword_hash_node().is_some() || arg.as_block_argument_node().is_some() {
                continue;
            }
            positional += 1;
        }
    }
    if has_splat_at_callsite {
        return None;
    }
    let min = arity.required;
    let max = if arity.has_rest {
        None
    } else {
        Some(arity.required + arity.optional)
    };
    let too_few = positional < min;
    let too_many = max.map(|m| positional > m).unwrap_or(false);
    if too_few || too_many {
        Some((min, max, positional))
    } else {
        None
    }
}

/// Information about the receiver of a method call
#[derive(Debug, Clone)]
enum ReceiverInfo {
    /// No receiver (e.g., `method_name`)
    NoReceiver,
    /// Self receiver (e.g., `self.method_name`)
    SelfReceiver,
    /// Constant receiver (e.g., `Foo.method` or `Foo::Bar.method`)
    ConstantReceiver(String),
    /// Expression receiver (e.g., `variable.method`)
    ExpressionReceiver,
    /// Invalid constant path (contains non-constant nodes)
    InvalidConstantPath,
}

impl ReferenceVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

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

        let mut index = self.index.lock();
        if !inference_failed {
            trace!(
                "Adding method call reference: {} at {:?}",
                method_fqn.to_string(),
                call_location
            );
            let caller_fqn = self.scope_tracker.current_method_fqn().cloned();
            index.add_reference(method_fqn.clone(), call_location, caller_fqn);
        } else {
            trace!(
                "Skipping reference for unresolved expression receiver: .{}",
                method_name
            );
        }

        // Track unresolved method calls if enabled (use message location for diagnostics)
        if self.track_unresolved {
            match &receiver_info {
                ReceiverInfo::NoReceiver => {
                    if !self.method_exists_in_index(
                        &index,
                        &method_name,
                        &target_namespace,
                        namespace_kind,
                    ) {
                        trace!("Adding unresolved method call: {}", method_name);
                        index.add_unresolved_entry(
                            self.document.uri.clone(),
                            UnresolvedEntry::method(method_name.clone(), None, message_location.clone()),
                        );
                    }
                }
                ReceiverInfo::ConstantReceiver(receiver_name) => {
                    if !self.method_exists_in_index(
                        &index,
                        &method_name,
                        &target_namespace,
                        namespace_kind,
                    ) {
                        trace!(
                            "Adding unresolved method call: {}.{}",
                            receiver_name,
                            method_name
                        );
                        index.add_unresolved_entry(
                            self.document.uri.clone(),
                            UnresolvedEntry::method(
                                method_name.clone(),
                                Some(RubyType::class(&receiver_name)),
                                message_location.clone(),
                            ),
                        );
                    }
                }
                ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath => {
                    // Only warn when receiver class is user-defined and method is missing.
                    // Skip if:
                    // - inferred type is unknown / non-class (downstream of broken chain),
                    // - receiver class isn't in the user index (stdlib/RBS-backed types
                    //   like String/Array — methods are defined in RBS, not user code).
                    if let Some(class_type @ (RubyType::Class(fqn) | RubyType::Module(fqn))) =
                        &inferred_expr_type
                    {
                        // Class/module entries are stored as Namespace(parts, Instance);
                        // type inference returns Constant(parts). Normalize before lookup.
                        let receiver_class_known_in_user_index = fqn
                            .to_instance_namespace()
                            .as_ref()
                            .map(|ns_fqn| index.contains_fqn(ns_fqn))
                            .unwrap_or(false);
                        if receiver_class_known_in_user_index {
                            let ns_parts = fqn.namespace_parts();
                            if !self.method_exists_in_index(
                                &index,
                                &method_name,
                                &ns_parts,
                                NamespaceKind::Instance,
                            ) {
                                trace!(
                                    "Adding unresolved method call on inferred type: .{}",
                                    method_name
                                );
                                index.add_unresolved_entry(
                                    self.document.uri.clone(),
                                    UnresolvedEntry::method(
                                        method_name.clone(),
                                        Some(class_type.clone()),
                                        message_location.clone(),
                                    ),
                                );
                            }
                        }
                    }
                }
                ReceiverInfo::SelfReceiver => {
                    // TODO: check method on current class (future work).
                }
            }
        }

        // Wrong-arity check (positional only). Skips splat callsites, kwargs, block.
        if self.track_unresolved {
            let owner_for_arity = match &receiver_info {
                ReceiverInfo::NoReceiver | ReceiverInfo::ConstantReceiver(_) => {
                    Some((target_namespace.clone(), namespace_kind))
                }
                ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath => {
                    // Same gating as unresolved-method on expr receivers: only
                    // when receiver class is user-defined (in the user index).
                    // Stdlib types (String/Array) are RBS-backed → skip.
                    if let Some(RubyType::Class(fqn) | RubyType::Module(fqn)) =
                        &inferred_expr_type
                    {
                        let known = fqn
                            .to_instance_namespace()
                            .as_ref()
                            .map(|ns_fqn| index.contains_fqn(ns_fqn))
                            .unwrap_or(false);
                        if known {
                            Some((fqn.namespace_parts(), NamespaceKind::Instance))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ReceiverInfo::SelfReceiver => None,
            };
            if let Some((owner, kind)) = owner_for_arity {
                if let Some(arity) =
                    self.find_method_arity_strict(&index, &method_name, &owner, kind)
                {
                    if let Some((min, max, actual)) = compute_arity_mismatch(node, &arity) {
                        index.add_unresolved_entry(
                            self.document.uri.clone(),
                            UnresolvedEntry::wrong_arity(
                                method_name.clone(),
                                min,
                                max,
                                actual,
                                message_location.clone(),
                            ),
                        );
                    }
                }
            }
        }
    }

    /// Strict ancestor-walk lookup. Returns the method's `(required, optional, has_rest)`
    /// arity tuple if and only if a single matching `MethodData` is found on the
    /// owner or one of its ancestors.
    fn find_method_arity_strict(
        &self,
        index: &crate::indexer::index::RubyIndex,
        method_name: &str,
        owner: &[RubyConstant],
        kind: NamespaceKind,
    ) -> Option<MethodArity> {
        let ruby_method = RubyMethod::new(method_name).ok()?;
        let entries = index.get_methods_by_name(&ruby_method)?;

        // Build owner + ancestors with namespace_kind, in resolution order.
        let mut search: Vec<FullyQualifiedName> = Vec::new();
        let owner_with_kind = FullyQualifiedName::namespace_with_kind(owner.to_vec(), kind);
        search.push(owner_with_kind.clone());
        for ancestor in index.get_ancestor_chain(&owner_with_kind) {
            let with_kind =
                FullyQualifiedName::namespace_with_kind(ancestor.namespace_parts(), kind);
            if !search.contains(&with_kind) {
                search.push(with_kind);
            }
        }

        for fqn in &search {
            for entry in &entries {
                if let EntryKind::Method(data) = &entry.kind {
                    if &data.owner == fqn {
                        return Some(MethodArity::from_params(&data.params));
                    }
                }
            }
        }
        None
    }

    /// Check if a method exists in the index
    fn method_exists_in_index(
        &self,
        index: &crate::indexer::index::RubyIndex,
        method_name: &str,
        target_namespace: &[RubyConstant],
        _namespace_kind: NamespaceKind,
    ) -> bool {
        // Create a method to check
        let method = match RubyMethod::new(method_name) {
            Ok(m) => m,
            Err(_) => return true, // If we can't create the method, assume it exists
        };

        // Check if the method exists by name (loose check)
        if index.contains_method(&method) {
            return true;
        }

        // Check the specific FQN
        let method_fqn = FullyQualifiedName::method(target_namespace.to_vec(), method.clone());
        if index.contains_fqn(&method_fqn) {
            return true;
        }

        // For methods without receiver, also check if it might be inherited
        // by checking the method name in any namespace
        if target_namespace.is_empty() {
            // Top-level method call - just check by name
            return false;
        }

        // Check parent namespaces for inherited methods
        let mut ancestors = target_namespace.to_vec();
        while !ancestors.is_empty() {
            if let Ok(m) = RubyMethod::new(method_name) {
                let fqn = FullyQualifiedName::method(ancestors.clone(), m);
                if index.contains_fqn(&fqn) {
                    return true;
                }
            }
            ancestors.pop();
        }

        false
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
    ) -> (Vec<RubyConstant>, NamespaceKind, ReceiverInfo, Option<RubyType>) {
        if receiver_node.as_self_node().is_some() {
            let (ns, kind) = self.handle_self_receiver(current_namespace);
            (ns, kind, ReceiverInfo::SelfReceiver, None)
        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
            let (ns, kind) = self.handle_constant_read_receiver(&constant_read, current_namespace);
            (ns, kind, ReceiverInfo::ConstantReceiver(name), None)
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            if self.is_valid_constant_path_receiver(receiver_node) {
                let receiver_name = self.build_constant_path_name(receiver_node);
                let (ns, kind) = self.handle_constant_path_receiver(
                    &constant_path,
                    receiver_node,
                    current_namespace,
                );
                (ns, kind, ReceiverInfo::ConstantReceiver(receiver_name), None)
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

    /// Build the full constant path name as a string (e.g., "Foo::Bar::Baz")
    fn build_constant_path_name(&self, node: &Node) -> String {
        let mut parts = Vec::new();
        self.collect_constant_path_parts_for_name(node, &mut parts);
        parts.join("::")
    }

    /// Recursively collect constant path parts for building the name
    fn collect_constant_path_parts_for_name(&self, node: &Node, parts: &mut Vec<String>) {
        if let Some(constant_path) = node.as_constant_path_node() {
            // Process parent first (left side)
            if let Some(parent) = constant_path.parent() {
                self.collect_constant_path_parts_for_name(&parent, parts);
            }
            // Then add the name (right side)
            if let Some(name_bytes) = constant_path.name() {
                let name = String::from_utf8_lossy(name_bytes.as_slice()).to_string();
                parts.push(name);
            }
        } else if let Some(constant_read) = node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
            parts.push(name);
        }
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
        let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
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
            let var_name = String::from_utf8_lossy(local_var.name().as_slice()).to_string();
            return self.infer_variable_type(&var_name);
        }

        // Case 2: Method call chain (e.g., `team.leader.name`)
        if let Some(call) = receiver_node.as_call_node() {
            let inner_method = String::from_utf8_lossy(call.name().as_slice()).to_string();

            // First resolve the inner receiver's type
            let inner_type = if let Some(inner_receiver) = call.receiver() {
                if let Some(constant_read) = inner_receiver.as_constant_read_node() {
                    // Constant receiver: Foo.bar → ClassReference(Foo)
                    let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
                    Some(RubyType::ClassReference(FullyQualifiedName::Constant(
                        vec![RubyConstant::new(&name).ok()?],
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

            // Now resolve the method's return type
            let index = self.index.lock();
            return MethodResolver::resolve_method_return_type(&index, &inner_type, &inner_method);
        }

        None
    }

    /// Infer a local variable's type from constructor patterns in the source.
    fn infer_variable_type(&self, var_name: &str) -> Option<RubyType> {
        let content = &self.document.content;
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(var_name) {
                let next_char = rest.chars().next();
                if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
                    continue;
                }
                let rest = rest.trim();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rhs = rest.trim();
                    if let Some(new_pos) = rhs.find(".new") {
                        let class_part = rhs[..new_pos].trim();
                        if !class_part.chars().next().is_some_and(|c| c.is_uppercase()) {
                            continue;
                        }

                        let parts: Vec<_> = class_part
                            .split("::")
                            .filter_map(|s| RubyConstant::new(s.trim()).ok())
                            .collect();
                        if parts.is_empty() {
                            continue;
                        }

                        return Some(RubyType::Class(FullyQualifiedName::Constant(parts)));
                    }
                }
            }
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
