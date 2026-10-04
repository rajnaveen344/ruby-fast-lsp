//! Method facts from DSL calls: attributes, visibility, aliases, dynamic
//! definitions, and delegation.

use crate::core::{
    FullyQualifiedName, MethodFact, MethodParamFact, MethodParamKind, MethodVisibility,
    MethodVisibilityOverrideFact, RubyConstant, RubyMethod, TextRange, TypeFact, TypeSubject,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::CallNode;

use super::syntax::{
    attr_name_and_range, call_two_symbol_or_string_args, define_method_name_and_range,
    delegate_methods_and_receiver, forwardable_delegates_and_receiver, symbol_name_and_range,
    text_range,
};
use super::AnalysisIndexer;
use crate::indexer::documents::scope_rules::{
    implicit_singleton_namespace, receiverless_definition_kind, sent_definition_kind,
};

impl AnalysisIndexer<'_> {
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
        self.facts.methods.push(
            MethodFact::new(fqn, owner, range).with_visibility(self.scope.current_visibility()),
        );
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
        self.facts.methods.push(
            MethodFact::with_param_facts(fqn, owner, range, params)
                .with_visibility(self.scope.current_visibility()),
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

        let owner_kind = self.owner_kind();

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = attr_name_and_range(&arg, self.file_id) else {
                continue;
            };

            if reader {
                if let Ok(method) = RubyMethod::new(&name) {
                    self.push_method_fact(self.owner_namespace(), owner_kind, method, range);
                }
            }

            if writer {
                if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                    self.push_method_fact_with_params(
                        self.owner_namespace(),
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
            .unwrap_or_else(|| self.owner_namespace());

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
            self.scope.enable_module_function_mode();
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            self.scope.enable_module_function_mode();
            return;
        };

        for arg in arguments.arguments().iter() {
            let Some((name, fallback_range)) = symbol_name_and_range(&arg, self.file_id) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.owner_namespace(), method);
            let instance_owner = FullyQualifiedName::namespace_with_kind(
                self.owner_namespace(),
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
                self.owner_namespace(),
                crate::core::NamespaceKind::Singleton,
            );
            // Ruby copies the method as a public singleton method and makes
            // the instance method private.
            self.facts
                .methods
                .push(MethodFact::new(fqn, owner, range).with_visibility(MethodVisibility::Public));
            self.set_method_visibility(method, MethodVisibility::Private, fallback_range);
        }
    }

    /// Give each method a `private`, `protected`, or `public` call names
    /// the call's visibility.
    pub(super) fn set_named_methods_visibility(
        &mut self,
        visibility: MethodVisibility,
        methods: Vec<(String, ruby_prism::Location<'_>)>,
    ) {
        for (name, location) in methods {
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            self.set_method_visibility(method, visibility, text_range(self.file_id, &location));
        }
    }

    fn set_method_visibility(
        &mut self,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) {
        let owner =
            FullyQualifiedName::namespace_with_kind(self.owner_namespace(), self.owner_kind());
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

        let owner_kind = self.owner_kind();
        let range = self.range(&node.location());
        self.push_method_fact_without_parameter_shape(
            self.owner_namespace(),
            owner_kind,
            new_method,
            range,
        );

        let old_fqn = FullyQualifiedName::method(self.owner_namespace(), old_method);
        let new_fqn = FullyQualifiedName::method(
            self.owner_namespace(),
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

    /// `define_method` and `define_singleton_method` are calls on `self`, so
    /// they define only where `self` is a class or module object; the
    /// collector applies the same rule.
    pub(super) fn push_define_method_fact(&mut self, node: &CallNode<'_>) {
        let Some((name, range)) = define_method_name_and_range(node, self.file_id, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        let Some(namespace) = implicit_singleton_namespace(&self.scope) else {
            return;
        };
        let Some(owner_kind) = receiverless_definition_kind(b"define_method", &self.scope) else {
            return;
        };
        self.push_method_fact_without_parameter_shape(namespace, owner_kind, method, range);
    }

    pub(super) fn push_define_singleton_method_fact(&mut self, node: &CallNode<'_>) {
        let Some(namespace) = implicit_singleton_namespace(&self.scope) else {
            return;
        };
        self.push_define_singleton_method_for_namespace(node, namespace);
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
        let Some(owner_kind) = sent_definition_kind(node.name().as_slice(), &selector) else {
            return;
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
        let owner_kind = self.owner_kind();
        let range = self.range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };
        for method_name in methods {
            let Ok(method) = RubyMethod::new(&method_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.owner_namespace(), method);
            let owner = FullyQualifiedName::namespace_with_kind(self.owner_namespace(), owner_kind);
            self.facts.methods.push(
                MethodFact::with_delegate_receiver(fqn, owner, range, receiver_method)
                    .with_visibility(self.scope.current_visibility()),
            );
        }
    }

    pub(super) fn push_forwardable_delegate_method_facts(&mut self, node: &CallNode<'_>) {
        let Some((receiver_method, methods)) =
            forwardable_delegates_and_receiver(node, self.file_id)
        else {
            return;
        };
        let owner_kind = self.owner_kind();
        let range = self.range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };
        for (defined_name, _target_name) in methods {
            let Ok(method) = RubyMethod::new(&defined_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.owner_namespace(), method);
            let owner = FullyQualifiedName::namespace_with_kind(self.owner_namespace(), owner_kind);
            self.facts.methods.push(
                MethodFact::with_delegate_receiver(fqn, owner, range, receiver_method)
                    .with_visibility(self.scope.current_visibility()),
            );
        }
    }
}
