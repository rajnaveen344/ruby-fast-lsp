use crate::core::{FullyQualifiedName, GraphEdgeKind, GraphNodeKind, NamespaceKind, RubyConstant};
use crate::indexer::LocalScopeKind as LVScopeKind;
use ruby_prism::{BlockNode, CallNode, NumberedParametersNode, ParametersNode};

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn process_block_node_entry(
        &mut self,
        node: &BlockNode,
    ) {
        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());
        self.scope_tracker.push_scope_kind(LVScopeKind::Block);
        self.document
            .variable_scopes_mut()
            .enter_scope(LVScopeKind::Block, body_range, None);
        self.assign_block_parameter_types(node);
    }

    pub(in crate::indexer::fact_collector) fn process_block_node_exit(
        &mut self,
        _node: &BlockNode,
    ) {
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }

    fn assign_block_parameter_types(&mut self, node: &BlockNode) {
        let Some(parameters) = node.parameters() else {
            return;
        };

        if let Some(params_node) = parameters
            .as_block_parameters_node()
            .and_then(|node| node.parameters())
        {
            self.assign_parameters_node_types(&params_node);
            return;
        }

        if let Some(numbered_params) = parameters.as_numbered_parameters_node() {
            self.assign_numbered_parameter_types(&numbered_params);
        }
    }

    fn assign_parameters_node_types(&mut self, params_node: &ParametersNode) {
        let mut positional_index = 0usize;

        for required in params_node.requireds().iter() {
            if let Some(param) = required.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.assign_current_block_parameter_type(
                    &param_name,
                    &param.location(),
                    positional_index,
                );
                positional_index += 1;
            }
        }

        for optional in params_node.optionals().iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.assign_current_block_parameter_type(
                    &param_name,
                    &param.location(),
                    positional_index,
                );
                positional_index += 1;
            }
        }

        if let Some(rest) = params_node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let Some(name) = param.name() {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    self.assign_current_block_parameter_type(
                        &param_name,
                        &param.location(),
                        positional_index,
                    );
                    positional_index += 1;
                }
            }
        }

        for post in params_node.posts().iter() {
            if let Some(param) = post.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.assign_current_block_parameter_type(
                    &param_name,
                    &param.location(),
                    positional_index,
                );
                positional_index += 1;
            }
        }
    }

    fn assign_numbered_parameter_types(&mut self, params_node: &NumberedParametersNode) {
        for index in 0..usize::from(params_node.maximum()) {
            let param_name = format!("_{}", index + 1);
            self.assign_current_block_parameter_type(&param_name, &params_node.location(), index);
        }
    }
}

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn static_eval_block_context(
        &self,
        node: &CallNode,
    ) -> Option<(Vec<RubyConstant>, NamespaceKind, NamespaceKind)> {
        let (implicit_receiver_kind, method_definition_kind) = match node.name().as_slice() {
            b"class_eval" | b"module_eval" | b"class_exec" | b"module_exec" => {
                (NamespaceKind::Singleton, NamespaceKind::Instance)
            }
            b"instance_eval" | b"instance_exec" => {
                (NamespaceKind::Singleton, NamespaceKind::Singleton)
            }
            _ => return None,
        };
        node.block()?;
        let namespace = match node.receiver() {
            None => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
                    .then_some(namespace)?
            }
            Some(receiver) if receiver.as_self_node().is_some() => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
                    .then_some(namespace)?
            }
            Some(receiver) => {
                let eval_ref = crate::indexer::mixin_ref_from_node(&receiver)?;
                self.resolve_static_eval_namespace(&eval_ref.parts, eval_ref.absolute)?
            }
        };
        Some((namespace, implicit_receiver_kind, method_definition_kind))
    }

    pub(in crate::indexer::fact_collector) fn static_dynamic_definition_block_context(
        &self,
        node: &CallNode,
    ) -> Option<(
        Vec<RubyConstant>,
        NamespaceKind,
        Vec<RubyConstant>,
        NamespaceKind,
    )> {
        node.block()?;
        let (definition_namespace, definition_kind) =
            self.scope_tracker.method_definition_context();
        let (implicit_namespace, implicit_kind) = match node.receiver() {
            None => {
                let target_kind = match node.name().as_slice() {
                    b"define_method"
                        if !self.scope_tracker.execution_context_active()
                            && self.scope_tracker.in_singleton() =>
                    {
                        NamespaceKind::Singleton
                    }
                    b"define_method" => NamespaceKind::Instance,
                    b"define_singleton_method" => NamespaceKind::Singleton,
                    _ => return None,
                };
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
                    return None;
                }
                (namespace, target_kind)
            }
            Some(receiver) if node.name().as_slice() == b"define_singleton_method" => (
                self.resolve_constant_receiver_namespace(&receiver)?,
                NamespaceKind::Singleton,
            ),
            Some(receiver)
                if matches!(
                    node.name().as_slice(),
                    b"send" | b"public_send" | b"__send__"
                ) =>
            {
                let arguments = node.arguments()?;
                let selector = arguments.arguments().iter().next()?;
                let target_kind = if let Some(symbol) = selector.as_symbol_node() {
                    match symbol.unescaped() {
                        b"define_method" => NamespaceKind::Instance,
                        b"define_singleton_method" => NamespaceKind::Singleton,
                        _ => return None,
                    }
                } else if let Some(string) = selector.as_string_node() {
                    match string.unescaped() {
                        b"define_method" => NamespaceKind::Instance,
                        b"define_singleton_method" => NamespaceKind::Singleton,
                        _ => return None,
                    }
                } else {
                    return None;
                };
                if node.name().as_slice() == b"public_send"
                    && target_kind == NamespaceKind::Instance
                {
                    return None;
                }
                (
                    self.resolve_constant_receiver_namespace(&receiver)?,
                    target_kind,
                )
            }
            Some(_) => return None,
        };
        Some((
            implicit_namespace,
            implicit_kind,
            definition_namespace,
            definition_kind,
        ))
    }

    pub(in crate::indexer::fact_collector) fn concern_class_methods_block_namespace(
        &mut self,
        node: &CallNode,
    ) -> Option<Vec<RubyConstant>> {
        if node.receiver().is_some() || node.name().as_slice() != b"class_methods" {
            return None;
        }
        node.block()?;

        let current_namespace = self.scope_tracker.get_ns_stack();
        if current_namespace.is_empty() {
            return None;
        }

        let class_methods = RubyConstant::new("ClassMethods").expect(
            "INVARIANT VIOLATED: static Concern ClassMethods constant is invalid. \
             This is a bug because `ClassMethods` is a valid Ruby constant. \
             Fix: inspect RubyConstant validation.",
        );
        let mut target_namespace = current_namespace.clone();
        target_namespace.push(class_methods);
        let target_fqn = FullyQualifiedName::namespace(target_namespace);
        let range = self.direct_range(&node.location());
        self.direct_push_namespace_facts(target_fqn, GraphNodeKind::Module, range, range);
        self.direct_push_edge(
            FullyQualifiedName::namespace(current_namespace),
            &[class_methods],
            false,
            GraphEdgeKind::Extend,
            range,
        );

        Some(vec![class_methods])
    }

    pub(in crate::indexer::fact_collector) fn resolve_static_eval_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<Vec<RubyConstant>> {
        if parts.is_empty() {
            return None;
        }

        let current_namespace = self.scope_tracker.get_ns_stack();
        if absolute {
            let fqn = FullyQualifiedName::namespace(parts.to_vec());
            return self.namespace_is_known(&fqn).then(|| parts.to_vec());
        }

        let mut search = current_namespace.clone();
        loop {
            let mut candidate = search.clone();
            candidate.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(candidate.clone());
            if self.namespace_is_known(&fqn) {
                return Some(candidate);
            }
            if search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.namespace_is_known(&fqn).then(|| parts.to_vec())
    }
}
