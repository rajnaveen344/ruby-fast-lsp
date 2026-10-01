//! Call receiver classification and receiver namespace/type resolution.

use crate::core::{FullyQualifiedName, GraphNodeKind, NamespaceKind, RubyConstant};
use crate::engine::{AnalysisQuery, VariableTypeKind};
use crate::indexer::{build_constant_path_name, mixin_ref_from_node, utf8_str};
use crate::invariant::ExpectInvariant;
use ruby_prism::Node;

use crate::core::RubyType;

use crate::indexer::fact_collector::FactCollector;

#[derive(Debug, Clone)]
pub(super) enum ReceiverInfo {
    NoReceiver,
    SelfReceiver,
    ConstantReceiver(String),
    ExpressionReceiver,
    InvalidConstantPath,
}

impl FactCollector {
    pub(super) fn handle_no_receiver(
        &self,
        _current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        self.scope_tracker.implicit_receiver_context()
    }

    pub(super) fn handle_receiver_node_with_info(
        &self,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (
        Vec<RubyConstant>,
        NamespaceKind,
        ReceiverInfo,
        Option<RubyType>,
    ) {
        if receiver_node.as_self_node().is_some() {
            let (namespace, kind) = self.scope_tracker.implicit_receiver_context();
            (namespace, kind, ReceiverInfo::SelfReceiver, None)
        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
            let name = utf8_str(constant_read.name().as_slice()).to_string();
            let (ns, kind, inferred) =
                self.handle_constant_read_receiver(&constant_read, current_namespace);
            (ns, kind, ReceiverInfo::ConstantReceiver(name), inferred)
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            if is_valid_constant_path_receiver(receiver_node) {
                let receiver_name = build_constant_path_name(receiver_node);
                let (ns, kind, inferred) = self.handle_constant_path_receiver(
                    &constant_path,
                    receiver_node,
                    current_namespace,
                );
                (
                    ns,
                    kind,
                    ReceiverInfo::ConstantReceiver(receiver_name),
                    inferred,
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

    fn handle_constant_read_receiver(
        &self,
        constant_read: &ruby_prism::ConstantReadNode,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind, Option<RubyType>) {
        let name = utf8_str(constant_read.name().as_slice());
        if let Ok(constant) = RubyConstant::new(name) {
            let mut lexical_namespace = current_namespace.to_vec();
            let mut unproven_value_constant = false;
            let value_type = loop {
                let mut parts = lexical_namespace.clone();
                parts.push(constant.clone());
                let constant_fqn = FullyQualifiedName::constant(parts);
                let namespace_fqn = FullyQualifiedName::namespace(constant_fqn.namespace_parts());
                let is_namespace = self
                    .facts
                    .analysis
                    .graph_nodes
                    .iter()
                    .any(|fact| fact.fqn == namespace_fqn)
                    || {
                        let engine = self.semantics.engine.read();
                        AnalysisQuery::new(&engine).has_graph_node(&namespace_fqn)
                    };
                if is_namespace {
                    return (
                        constant_fqn.namespace_parts(),
                        NamespaceKind::Singleton,
                        None,
                    );
                }
                if let Some(ruby_type) = self.direct_constant_value_type(&constant_fqn) {
                    break Some(ruby_type);
                }
                if self.direct_constant_has_value(&constant_fqn) {
                    unproven_value_constant = true;
                    break None;
                }
                if lexical_namespace.pop().is_none() {
                    break None;
                }
            }
            .or_else(|| {
                if unproven_value_constant {
                    return None;
                }
                let engine = self.semantics.engine.read();
                let query = AnalysisQuery::new(&engine);
                query
                    .resolve_constant_in_context(std::slice::from_ref(&constant), current_namespace)
                    .and_then(|resolved| {
                        query.constant_value_type(&FullyQualifiedName::constant(
                            resolved.namespace_parts(),
                        ))
                    })
            });
            if let Some(ref ruby_type) = value_type {
                if let Some(namespace) = self.type_to_namespace_parts(ruby_type) {
                    let kind = match ruby_type {
                        RubyType::ClassReference(_) | RubyType::ModuleReference(_) => {
                            NamespaceKind::Singleton
                        }
                        RubyType::Class(_) | RubyType::Module(_) => NamespaceKind::Instance,
                        RubyType::Array(_)
                        | RubyType::Hash(_, _)
                        | RubyType::Literal(_)
                        | RubyType::Shape(_)
                        | RubyType::Union(_)
                        | RubyType::Unknown => {
                            let engine = self.semantics.engine.read();
                            AnalysisQuery::new(&engine)
                                .type_to_namespace(ruby_type)
                                .and_then(|fqn| fqn.namespace_kind())
                                .unwrap_or(NamespaceKind::Instance)
                        }
                    };
                    return (namespace, kind, value_type);
                }
            }
            if unproven_value_constant {
                // The constant holds an object whose type is not proven, such
                // as a class built by a factory method. Its receiver is
                // unknown, not the constant's own singleton namespace.
                return (
                    current_namespace.to_vec(),
                    NamespaceKind::Instance,
                    Some(RubyType::Unknown),
                );
            }
            let mut receiver_namespace = current_namespace.to_vec();
            receiver_namespace.push(constant);
            (receiver_namespace, NamespaceKind::Singleton, None)
        } else {
            (current_namespace.to_vec(), NamespaceKind::Instance, None)
        }
    }

    fn handle_constant_path_receiver(
        &self,
        _constant_path: &ruby_prism::ConstantPathNode,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind, Option<RubyType>) {
        if let Some(reference) = mixin_ref_from_node(receiver_node) {
            let lexical_context = self.scope_tracker.get_ns_stack();
            if let Some((_constant, ruby_type)) = self.resolve_constant_value_type_from(
                &reference.parts,
                reference.absolute,
                &lexical_context,
            ) {
                if let Some(namespace) = self.type_to_namespace_parts(&ruby_type) {
                    let kind = match ruby_type {
                        RubyType::ClassReference(_) | RubyType::ModuleReference(_) => {
                            NamespaceKind::Singleton
                        }
                        RubyType::Class(_)
                        | RubyType::Module(_)
                        | RubyType::Literal(_)
                        | RubyType::Array(_)
                        | RubyType::Hash(_, _)
                        | RubyType::Shape(_)
                        | RubyType::Union(_)
                        | RubyType::Unknown => NamespaceKind::Instance,
                    };
                    return (namespace, kind, Some(ruby_type));
                }
            }
        }

        if let Some(mixin_ref) = mixin_ref_from_node(receiver_node) {
            let context = if mixin_ref.absolute {
                Vec::new()
            } else {
                current_namespace.to_vec()
            };
            if let Some(resolved_fqn) =
                self.direct_resolve_namespace(&mixin_ref.parts, mixin_ref.absolute)
            {
                return (
                    resolved_fqn.namespace_parts(),
                    NamespaceKind::Singleton,
                    None,
                );
            }
            if let Some(resolved_fqn) =
                self.resolve_constant_from_analysis(&mixin_ref.parts, &context)
            {
                return (
                    resolved_fqn.namespace_parts(),
                    NamespaceKind::Singleton,
                    None,
                );
            }
        }

        if let Some(mixin_ref) = mixin_ref_from_node(receiver_node) {
            (mixin_ref.parts, NamespaceKind::Singleton, None)
        } else {
            (current_namespace.to_vec(), NamespaceKind::Instance, None)
        }
    }

    fn handle_expression_receiver(
        &self,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind, Option<RubyType>) {
        let inferred = self.infer_expression_receiver_type(receiver_node);
        if let Some(ref resolved_type) = inferred {
            if let Some(ns) = self.type_to_namespace_parts(resolved_type) {
                let kind = match resolved_type {
                    RubyType::ClassReference(_) | RubyType::ModuleReference(_) => {
                        NamespaceKind::Singleton
                    }
                    RubyType::Class(_)
                    | RubyType::Module(_)
                    | RubyType::Literal(_)
                    | RubyType::Array(_)
                    | RubyType::Hash(_, _)
                    | RubyType::Shape(_)
                    | RubyType::Union(_)
                    | RubyType::Unknown => NamespaceKind::Instance,
                };
                return (ns, kind, Some(resolved_type.clone()));
            }
        }

        (
            current_namespace.to_vec(),
            NamespaceKind::Instance,
            inferred,
        )
    }

    fn infer_expression_receiver_type(&self, receiver_node: &Node) -> Option<RubyType> {
        if !self.options.infer_expression_receivers {
            return None;
        }

        // Local/ivar receivers never receive `TypeSubject::Expression` facts (those are
        // recorded for call/expression ranges). Resolve them before consulting the
        // exact expression range index.
        if let Some(local_var) = receiver_node.as_local_variable_read_node() {
            let var_name = utf8_str(local_var.name().as_slice());
            return self.get_local_var_type(var_name, &local_var.location());
        }

        if let Some(ivar) = receiver_node.as_instance_variable_read_node() {
            let var_name = utf8_str(ivar.name().as_slice());
            let byte_offset = u32::try_from(ivar.location().start_offset()).expect_invariant(
                "Prism location offset exceeded u32",
                "ruby-analysis::core TextRange currently stores u32 offsets",
                "widen TextRange offsets before indexing files larger than u32::MAX bytes",
            );
            let owner = FullyQualifiedName::namespace_with_kind(
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.current_method_context(),
            );
            return self.collected_nonlocal_variable_type_before(
                VariableTypeKind::Instance,
                var_name,
                &owner,
                byte_offset,
            );
        }

        if let Some(class_var) = receiver_node.as_class_variable_read_node() {
            let var_name = utf8_str(class_var.name().as_slice());
            let byte_offset = u32::try_from(class_var.location().start_offset()).expect_invariant(
                "Prism location offset exceeded u32",
                "ruby-analysis::core TextRange currently stores u32 offsets",
                "widen TextRange offsets before indexing files larger than u32::MAX bytes",
            );
            let owner = FullyQualifiedName::namespace_with_kind(
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.current_method_context(),
            );
            return self.collected_nonlocal_variable_type_before(
                VariableTypeKind::Class,
                var_name,
                &owner,
                byte_offset,
            );
        }

        if let Some(global_var) = receiver_node.as_global_variable_read_node() {
            let var_name = utf8_str(global_var.name().as_slice());
            let byte_offset = u32::try_from(global_var.location().start_offset()).expect_invariant(
                "Prism location offset exceeded u32",
                "ruby-analysis::core TextRange currently stores u32 offsets",
                "widen TextRange offsets before indexing files larger than u32::MAX bytes",
            );
            let owner = FullyQualifiedName::namespace_with_kind(
                self.scope_tracker.get_ns_stack(),
                self.scope_tracker.current_method_context(),
            );
            return self.collected_nonlocal_variable_type_before(
                VariableTypeKind::Global,
                var_name,
                &owner,
                byte_offset,
            );
        }

        let expression_range = self.direct_range(&receiver_node.location());
        if let Some(fact) = self.direct_expression_fact(expression_range, None) {
            return Some(fact.ruby_type.clone());
        }

        // Reuse the ordinary expression inference below for a call receiver.
        // That path understands structural Hash reads and retains their exact
        // proof outcome. Reconstructing the nested call from only its method
        // name and receiver type would route `payload[:profile][:name]`
        // through generic method lookup and erase the inner keyed-read proof.
        Some(self.infer_type_from_value(receiver_node)).filter(|ty| *ty != RubyType::Unknown)
    }

    pub(super) fn proven_namespace_receiver_type(
        &self,
        namespace_parts: &[RubyConstant],
    ) -> Option<RubyType> {
        let namespace = FullyQualifiedName::namespace(namespace_parts.to_vec());
        let kind = self
            .facts
            .analysis
            .graph_nodes
            .iter()
            .filter(|fact| fact.fqn == namespace)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.kind)
            .or_else(|| {
                let engine = self.semantics.engine.read();
                AnalysisQuery::new(&engine).namespace_node_kind(&namespace)
            })?;
        let constant = FullyQualifiedName::constant(namespace_parts.to_vec());
        Some(match kind {
            GraphNodeKind::Class => RubyType::ClassReference(constant),
            GraphNodeKind::Module => RubyType::ModuleReference(constant),
        })
    }

    fn type_to_namespace_parts(&self, ruby_type: &RubyType) -> Option<Vec<RubyConstant>> {
        match ruby_type {
            RubyType::Class(fqn)
            | RubyType::ClassReference(fqn)
            | RubyType::Module(fqn)
            | RubyType::ModuleReference(fqn) => return Some(fqn.namespace_parts()),
            RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Literal(_)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => {}
        }
        let engine = self.semantics.engine.read();
        AnalysisQuery::new(&engine)
            .type_to_namespace(ruby_type)
            .map(|namespace| namespace.namespace_parts())
    }

    pub(super) fn resolve_constant_from_analysis(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let engine = self.semantics.engine.read();
        crate::engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(parts, current_namespace)
    }
}

fn is_valid_constant_path_receiver(node: &Node) -> bool {
    if node.as_constant_read_node().is_some() {
        return true;
    }

    if let Some(constant_path) = node.as_constant_path_node() {
        if let Some(parent) = constant_path.parent() {
            return is_valid_constant_path_receiver(&parent);
        }
        return true;
    }

    false
}
