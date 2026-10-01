//! Method facts from DSL calls: attributes, visibility, aliases, dynamic
//! definitions, and delegation.

use crate::core::{
    FullyQualifiedName, MethodFact, MethodParamFact, MethodParamKind, MethodVisibility,
    MethodVisibilityOverrideFact, NamespaceKind, RubyConstant, RubyMethod, SymbolFact, SymbolKind,
    TextRange, TypeFact, TypeSubject,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::CallNode;

use super::syntax::{
    attr_name_and_range, call_two_symbol_or_string_args, define_method_name_and_range,
    delegate_methods_and_receiver, forwardable_delegates_and_receiver, method_name_and_range,
    symbol_name_and_range,
};
use super::{AnalysisIndexer, ScopeKind};

impl AnalysisIndexer {
    fn push_method_fact(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: crate::core::NamespaceKind,
        method: RubyMethod,
        range: TextRange,
    ) {
        self.push_method_fact_with_params(namespace, owner_kind, method, range, Vec::new());
    }

    pub(super) fn push_method_fact_without_parameter_shape(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: crate::core::NamespaceKind,
        method: RubyMethod,
        range: TextRange,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.facts
            .methods
            .push(MethodFact::new(fqn, owner, range).with_visibility(self.current_visibility()));
    }

    fn push_method_fact_with_params(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: crate::core::NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        params: Vec<MethodParamFact>,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.facts.methods.push(
            MethodFact::with_param_facts(fqn, owner, range, params)
                .with_visibility(self.current_visibility()),
        );
    }

    pub(super) fn push_attr_method_facts(
        &mut self,
        node: &CallNode<'_>,
        reader: bool,
        writer: bool,
    ) {
        let Some(arguments) = node.arguments() else {
            return;
        };

        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = attr_name_and_range(&arg, self.file_id) else {
                continue;
            };

            if reader {
                if let Ok(method) = RubyMethod::new(&name) {
                    self.push_method_fact(self.namespace_stack.clone(), owner_kind, method, range);
                }
            }

            if writer {
                if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                    self.push_method_fact_with_params(
                        self.namespace_stack.clone(),
                        owner_kind,
                        method,
                        range,
                        vec![MethodParamFact::new("value", MethodParamKind::Required)],
                    );
                }
            }
        }
    }

    pub(super) fn push_class_attribute_method_facts(&mut self, node: &CallNode<'_>) {
        let Some(arguments) = node.arguments() else {
            return;
        };

        let namespace = node
            .receiver()
            .and_then(|receiver| self.resolve_constant_receiver_namespace(&receiver))
            .unwrap_or_else(|| self.namespace_stack.clone());

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = attr_name_and_range(&arg, self.file_id) else {
                continue;
            };

            if let Ok(method) = RubyMethod::new(&name) {
                self.push_method_fact(
                    namespace.clone(),
                    crate::core::NamespaceKind::Singleton,
                    method,
                    range,
                );
                self.push_method_fact(
                    namespace.clone(),
                    crate::core::NamespaceKind::Instance,
                    method,
                    range,
                );
            }

            if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                let params = vec![MethodParamFact::new("value", MethodParamKind::Required)];
                self.push_method_fact_with_params(
                    namespace.clone(),
                    crate::core::NamespaceKind::Singleton,
                    method,
                    range,
                    params.clone(),
                );
                self.push_method_fact_with_params(
                    namespace.clone(),
                    crate::core::NamespaceKind::Instance,
                    method,
                    range,
                    params,
                );
            }
        }
    }

    pub(super) fn push_module_function_facts(&mut self, node: &CallNode<'_>) {
        let Some(arguments) = node.arguments() else {
            if let Some(mode) = self.module_function_mode_stack.last_mut() {
                *mode = true;
            }
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            if let Some(mode) = self.module_function_mode_stack.last_mut() {
                *mode = true;
            }
            return;
        };

        for arg in arguments.arguments().iter() {
            let Some((name, fallback_range)) = symbol_name_and_range(&arg, self.file_id) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
            let instance_owner = FullyQualifiedName::namespace_with_kind(
                self.namespace_stack.clone(),
                crate::core::NamespaceKind::Instance,
            );
            let range = self
                .facts
                .methods
                .iter()
                .find(|fact| fact.fqn == fqn && fact.owner == instance_owner)
                .map(|fact| fact.range)
                .unwrap_or(fallback_range);
            let owner = FullyQualifiedName::namespace_with_kind(
                self.namespace_stack.clone(),
                crate::core::NamespaceKind::Singleton,
            );
            self.facts.methods.push(
                MethodFact::new(fqn, owner, range).with_visibility(self.current_visibility()),
            );
        }
    }

    pub(super) fn push_visibility_modifier(
        &mut self,
        node: &CallNode<'_>,
        visibility: MethodVisibility,
    ) {
        let Some(arguments) = node.arguments() else {
            self.set_current_visibility(visibility);
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            self.set_current_visibility(visibility);
            return;
        }

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = method_name_and_range(&arg, self.file_id) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            self.set_method_visibility(method, visibility, range);
        }
    }

    fn set_method_visibility(
        &mut self,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) {
        let owner = FullyQualifiedName::namespace_with_kind(
            self.namespace_stack.clone(),
            match self.current_scope_kind() {
                ScopeKind::Instance => NamespaceKind::Instance,
                ScopeKind::Singleton => NamespaceKind::Singleton,
            },
        );
        self.facts
            .method_visibility_overrides
            .push(MethodVisibilityOverrideFact::new(
                owner.clone(),
                method,
                visibility,
                range,
            ));
        for fact in &mut self.facts.methods {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                continue;
            };
            if *fact_method == method && fact.owner == owner {
                fact.visibility = visibility;
            }
        }
    }

    pub(super) fn push_alias_method_call_fact(&mut self, node: &CallNode<'_>) {
        let Some((new_name, old_name)) = call_two_symbol_or_string_args(node, self.file_id) else {
            return;
        };
        let Ok(new_method) = RubyMethod::new(&new_name) else {
            return;
        };
        let Ok(old_method) = RubyMethod::new(&old_name) else {
            return;
        };

        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        self.push_method_fact_without_parameter_shape(
            self.namespace_stack.clone(),
            owner_kind,
            new_method,
            range,
        );

        let old_fqn = FullyQualifiedName::method(self.namespace_stack.clone(), old_method);
        let new_fqn = FullyQualifiedName::method(
            self.namespace_stack.clone(),
            RubyMethod::new(&new_name).expect_invariant(
                "alias_method new method became invalid after validation",
                "the same string was already accepted",
                "keep alias_method validation single-sourced",
            ),
        );
        if let Some(old_type) = self
            .facts
            .types
            .iter()
            .find(|fact| fact.subject == TypeSubject::MethodReturn(old_fqn.clone()))
            .cloned()
        {
            self.facts.types.push(TypeFact::new(
                TypeSubject::MethodReturn(new_fqn),
                old_type.ruby_type,
                range,
                old_type.provenance,
            ));
        }
    }

    pub(super) fn push_define_method_fact(&mut self, node: &CallNode<'_>) {
        let Some((name, range)) = define_method_name_and_range(node, self.file_id, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        if self.namespace_stack.is_empty() {
            return;
        }
        let owner_kind = if self.eval_context_active() {
            crate::core::NamespaceKind::Instance
        } else if self.current_scope_kind() == ScopeKind::Singleton {
            crate::core::NamespaceKind::Singleton
        } else if let Some((_method, method_kind)) = self.method_context_stack.last() {
            if *method_kind == NamespaceKind::Instance {
                return;
            }
            crate::core::NamespaceKind::Instance
        } else {
            crate::core::NamespaceKind::Instance
        };
        self.push_method_fact_without_parameter_shape(
            self.namespace_stack.clone(),
            owner_kind,
            method,
            range,
        );
    }

    pub(super) fn push_define_singleton_method_fact(&mut self, node: &CallNode<'_>) {
        if self.namespace_stack.is_empty() || !self.method_context_stack.is_empty() {
            return;
        }
        self.push_define_singleton_method_for_namespace(node, self.namespace_stack.clone());
    }

    pub(super) fn push_receiver_define_singleton_method_fact(&mut self, node: &CallNode<'_>) {
        if node.name().as_slice() != b"define_singleton_method" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(namespace) = self.resolve_constant_receiver_namespace(&receiver) else {
            return;
        };
        self.push_define_singleton_method_for_namespace(node, namespace);
    }

    fn push_define_singleton_method_for_namespace(
        &mut self,
        node: &CallNode<'_>,
        namespace: Vec<RubyConstant>,
    ) {
        let Some((name, range)) = define_method_name_and_range(node, self.file_id, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        self.push_method_fact_without_parameter_shape(
            namespace,
            crate::core::NamespaceKind::Singleton,
            method,
            range,
        );
    }

    pub(super) fn push_send_dynamic_method_fact(&mut self, node: &CallNode<'_>) {
        if !matches!(
            node.name().as_slice(),
            b"send" | b"public_send" | b"__send__"
        ) {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut args = arguments.arguments().iter();
        let Some((selector, _)) = args
            .next()
            .and_then(|arg| attr_name_and_range(&arg, self.file_id))
        else {
            return;
        };
        let owner_kind = match selector.as_str() {
            "define_method" if node.name().as_slice() != b"public_send" => {
                crate::core::NamespaceKind::Instance
            }
            "define_singleton_method" => crate::core::NamespaceKind::Singleton,
            _ => return,
        };
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(namespace) = self.resolve_constant_receiver_namespace(&receiver) else {
            return;
        };
        let Some((name, range)) = define_method_name_and_range(node, self.file_id, 1) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        self.push_method_fact_without_parameter_shape(namespace, owner_kind, method, range);
    }

    pub(super) fn push_delegate_method_facts(&mut self, node: &CallNode<'_>) {
        let Some((methods, receiver_method)) = delegate_methods_and_receiver(node, self.file_id)
        else {
            return;
        };
        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };
        for method_name in methods {
            let Ok(method) = RubyMethod::new(&method_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
            let owner =
                FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
            self.facts
                .symbols
                .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
            self.facts.methods.push(
                MethodFact::with_delegate_receiver(fqn, owner, range, receiver_method)
                    .with_visibility(self.current_visibility()),
            );
        }
    }

    pub(super) fn push_forwardable_delegate_method_facts(&mut self, node: &CallNode<'_>) {
        let Some((receiver_method, methods)) =
            forwardable_delegates_and_receiver(node, self.file_id)
        else {
            return;
        };
        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };
        for (defined_name, _target_name) in methods {
            let Ok(method) = RubyMethod::new(&defined_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
            let owner =
                FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
            self.facts
                .symbols
                .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
            self.facts.methods.push(
                MethodFact::with_delegate_receiver(fqn, owner, range, receiver_method)
                    .with_visibility(self.current_visibility()),
            );
        }
    }
}
