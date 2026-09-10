use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, RubyType, TypeInferenceOutcome,
    UnknownReason,
};
use crate::engine::AnalysisQuery;
use crate::inference::r#type::literal::project_immediate_hash_receiver_type;
use crate::inference::r#type::shape as shape_reads;
use crate::inference::type_tracker::flow::shapes::values::type_is_shape_only;
use crate::inference::type_tracker::returns::dependencies::call_is_direct_recursive;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

impl TypeTracker {
    /// Infer the return type of a method call
    pub(in crate::inference::type_tracker) fn infer_call(&mut self, call: &CallNode) -> RubyType {
        self.invalidate_escaped_callables_in_call(call);
        let method_name = String::from_utf8_lossy(call.name().as_slice()).to_string();

        if let Some(higher_order_type) = self.infer_rbs_higher_order_call(call, &method_name) {
            self.returns
                .direct_call_proofs
                .insert(call.location().start_offset());
            return higher_order_type;
        }

        self.apply_array_shape_call_boundary(call, &method_name);
        // Shape effects are flow state, not ordinary Hash method-return
        // lookup. Apply them before resolving the remaining call result so
        // every later alias read observes the same abstract identity state.
        if let Some(result) = self.infer_shape_call_effect(call, &method_name) {
            return result;
        }

        // A statically modeled yielding/proc call proves the block result
        // directly. Do not replace that proof with the callee's ordinary
        // method-return equation: Ruby yielding APIs intentionally return the
        // block value even when their own body contains an otherwise unknown
        // `yield` expression.
        if let Some(block_return_type) = self.infer_yielding_block_return_type(call, &method_name) {
            self.returns
                .direct_call_proofs
                .insert(call.location().start_offset());
            return block_return_type;
        }
        if let Some(proc_return_type) = self.infer_proc_call_return_type(call, &method_name) {
            self.returns
                .direct_call_proofs
                .insert(call.location().start_offset());
            return proc_return_type;
        }

        if call_is_direct_recursive(call, self.context.method.as_ref()) {
            self.returns.saw_direct_recursion = true;
            if let Some(approximation) = self.returns.approximation.as_ref() {
                return approximation.as_ruby_type();
            }
        }

        // Handle .new specially - it returns an instance of the class
        if method_name == "new" {
            if let Some(receiver) = call.receiver() {
                if let Some(const_read) = receiver.as_constant_read_node() {
                    let class_name =
                        String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                    if let Ok(constant) = RubyConstant::new(&class_name) {
                        let fqn = FullyQualifiedName::constant(vec![constant]);
                        return RubyType::Class(fqn);
                    }
                }
                // Handle namespaced constant like Foo::Bar.new
                if let Some(const_path) = receiver.as_constant_path_node() {
                    if let Some(fqn) = Self::resolve_constant_path(&const_path) {
                        return RubyType::Class(fqn);
                    }
                }
            }
        }

        // Get receiver type
        let mut receiver_type = if let Some(receiver) = call.receiver() {
            let inferred = self.infer_expression(&receiver);
            project_immediate_hash_receiver_type(&receiver, inferred)
        } else {
            // Implicit self - use current class context
            if let Some(ref fqn) = self.context.class {
                RubyType::Class(fqn.clone())
            } else {
                RubyType::Unknown
            }
        };

        // If receiver is Unknown, propagate Unknown (no global lookup)
        if receiver_type == RubyType::Unknown {
            return RubyType::Unknown;
        }

        if type_is_shape_only(&receiver_type) {
            if let Some(return_type) =
                self.infer_shape_read_from_type(call, &method_name, &receiver_type)
            {
                return return_type;
            }
            receiver_type =
                shape_reads::generic_hash_projection(&receiver_type).unwrap_or(RubyType::Unknown);
            if receiver_type == RubyType::Unknown {
                return RubyType::Unknown;
            }
        }

        let allow_private = call.receiver().is_none();
        if let RubyType::Union(members) = &receiver_type {
            return crate::inference::method::return_type::resolve_proven_union(
                members,
                |member| {
                    self.resolve_method_return_type_from_analysis(
                        member,
                        &method_name,
                        allow_private,
                    )
                    .or_else(|| self.resolve_rbs_method_return_type(member, &method_name))
                },
            )
            .unwrap_or(RubyType::Unknown);
        }
        if let Some(return_type) = self.resolve_method_return_type_from_analysis(
            &receiver_type,
            &method_name,
            allow_private,
        ) {
            return return_type;
        }

        self.resolve_rbs_method_return_type(&receiver_type, &method_name)
            .unwrap_or(RubyType::Unknown)
    }

    pub(in crate::inference::type_tracker) fn resolve_static_method_return_outcome(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> TypeInferenceOutcome {
        if let RubyType::Union(members) = receiver_type {
            let mut return_types = Vec::with_capacity(members.len());
            for member in members {
                let outcome = self.resolve_static_method_return_outcome(member, method_name);
                let Some(return_type) = outcome.into_proven_type() else {
                    return TypeInferenceOutcome::unknown(UnknownReason::IncompleteUnionMember);
                };
                return_types.push(return_type);
            }
            return TypeInferenceOutcome::from_optional(
                (!return_types.is_empty()).then(|| RubyType::union(return_types)),
                UnknownReason::IncompleteUnionMember,
            );
        }
        TypeInferenceOutcome::from_optional(
            self.resolve_method_return_type_from_analysis(receiver_type, method_name, false)
                .or_else(|| self.resolve_rbs_method_return_type(receiver_type, method_name)),
            UnknownReason::UnresolvedMethodReturn,
        )
    }

    pub(in crate::inference::type_tracker) fn resolve_rbs_method_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let is_singleton = matches!(
            receiver_type,
            RubyType::ClassReference(_) | RubyType::ModuleReference(_)
        );
        let class_name = match receiver_type {
            RubyType::Class(fqn)
            | RubyType::ClassReference(fqn)
            | RubyType::Module(fqn)
            | RubyType::ModuleReference(fqn) => fqn.namespace_parts().last().map(|c| c.to_string()),
            RubyType::Array(_) => Some("Array".to_string()),
            RubyType::Hash(_, _) => Some("Hash".to_string()),
            RubyType::Shape(shape) => {
                return self.resolve_rbs_method_return_type(&shape.generic_hash_type(), method_name)
            }
            RubyType::Literal(value) => {
                return self.resolve_rbs_method_return_type(&value.widened_type(), method_name)
            }
            RubyType::Union(_) | RubyType::Unknown => None,
        }?;
        crate::inference::rbs::get_rbs_method_return_type_as_ruby_type(
            &class_name,
            method_name,
            is_singleton,
        )
    }

    pub(in crate::inference::type_tracker) fn resolve_method_return_type_from_analysis(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
        allow_private: bool,
    ) -> Option<RubyType> {
        let analysis_engine = self.analysis.engine.as_ref()?;
        let method = crate::core::RubyMethod::new(method_name).ok()?;
        if let Some(return_type) =
            self.local_method_return_type_for_receiver(receiver_type, &method, !allow_private)
        {
            return Some(return_type);
        }
        let (receiver_fqn, namespace_kind) = match receiver_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                (fqn.clone(), crate::core::NamespaceKind::Instance)
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                (fqn.clone(), crate::core::NamespaceKind::Singleton)
            }
            RubyType::Literal(value) => {
                return self.resolve_method_return_type_from_analysis(
                    &value.widened_type(),
                    method_name,
                    allow_private,
                );
            }
            RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => {
                return None;
            }
        };
        let namespace =
            FullyQualifiedName::namespace_with_kind(receiver_fqn.namespace_parts(), namespace_kind);
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        if allow_private {
            self.analysis.query_cache.as_ref().map_or_else(
                || query.method_return_type_for_receiver(&namespace, &method),
                |cache| query.method_return_type_for_receiver_cached(&namespace, &method, cache),
            )
        } else if let Some(current_class) = self.context.class.as_ref() {
            let caller_namespace = FullyQualifiedName::namespace_with_kind(
                current_class.namespace_parts(),
                crate::core::NamespaceKind::Instance,
            );
            self.analysis.query_cache.as_ref().map_or_else(
                || {
                    query.method_return_type_for_protected_receiver(
                        &namespace,
                        &method,
                        &caller_namespace,
                    )
                },
                |cache| {
                    query.method_return_type_for_protected_receiver_cached(
                        &namespace,
                        &method,
                        &caller_namespace,
                        cache,
                    )
                },
            )
        } else {
            self.analysis.query_cache.as_ref().map_or_else(
                || query.method_return_type_for_public_receiver(&namespace, &method),
                |cache| {
                    query.method_return_type_for_public_receiver_cached(&namespace, &method, cache)
                },
            )
        }
    }

    pub(in crate::inference::type_tracker) fn local_method_return_type_for_receiver(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        require_public: bool,
    ) -> Option<RubyType> {
        let parts = match receiver_type {
            RubyType::Class(fqn)
            | RubyType::Module(fqn)
            | RubyType::ClassReference(fqn)
            | RubyType::ModuleReference(fqn) => fqn.namespace_parts(),
            RubyType::Literal(_) | RubyType::Shape(_) => return None,
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Union(_) | RubyType::Unknown => {
                return None;
            }
        };
        let method_fqn = FullyQualifiedName::method(parts, method.clone());
        if require_public && !self.analysis.public_methods.contains(&method_fqn) {
            return None;
        }
        self.analysis.method_returns.get(&method_fqn).cloned()
    }

    pub(in crate::inference::type_tracker) fn infer_super_return_type(&self) -> RubyType {
        let Some(current_class) = self.context.class.as_ref() else {
            return RubyType::Unknown;
        };
        let Some(method) = self.context.method.as_ref() else {
            return RubyType::Unknown;
        };
        let namespace = FullyQualifiedName::namespace_with_kind(
            current_class.namespace_parts(),
            NamespaceKind::Instance,
        );

        if let Some(superclass) = self.analysis.superclasses.get(&namespace) {
            let super_method =
                FullyQualifiedName::method(superclass.namespace_parts(), method.clone());
            if let Some(return_type) = self.analysis.method_returns.get(&super_method) {
                return return_type.clone();
            }
        }

        let Some(analysis_engine) = self.analysis.engine.as_ref() else {
            return RubyType::Unknown;
        };
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let Some(callee) = query.resolve_super_method_callee(&namespace, method) else {
            return RubyType::Unknown;
        };
        let super_method =
            FullyQualifiedName::method(callee.owner.namespace_parts(), method.clone());
        if let Some(return_type) = self.analysis.method_returns.get(&super_method) {
            return return_type.clone();
        }
        query
            .method_return_type_for_callee(&callee)
            .unwrap_or(RubyType::Unknown)
    }

    pub(in crate::inference::type_tracker) fn implicit_self_call_fqn(
        &self,
        call: &CallNode<'_>,
    ) -> Option<FullyQualifiedName> {
        if call
            .receiver()
            .is_some_and(|receiver| receiver.as_self_node().is_none())
        {
            return None;
        }
        let method_name = String::from_utf8_lossy(call.name().as_slice());
        let method = RubyMethod::new(method_name.as_ref()).ok()?;
        Some(FullyQualifiedName::method(
            self.context
                .class
                .as_ref()
                .map(FullyQualifiedName::namespace_parts)
                .unwrap_or_default(),
            method,
        ))
    }
}
