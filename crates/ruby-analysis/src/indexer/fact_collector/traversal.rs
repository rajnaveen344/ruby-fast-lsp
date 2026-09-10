use super::FactCollector;
use crate::inference::control_flow;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

impl Visit<'_> for FactCollector {
    fn visit_case_node(&mut self, node: &CaseNode<'_>) {
        if let Some(predicate) = node.predicate() {
            self.visit(&predicate);
        }
        let before = self.flow.local_callables.clone();
        let mut surviving = Vec::new();
        for condition in node.conditions().iter() {
            let when = condition.as_when_node().expect(
                "INVARIANT VIOLATED: an ordinary CaseNode contains a non-When condition. This is a bug because Prism's CaseNode schema permits only WhenNode conditions. Fix: route pattern cases through CaseMatchNode instead of weakening this invariant.",
            );
            self.flow.local_callables = before.clone();
            for expression in when.conditions().iter() {
                self.visit(&expression);
            }
            if let Some(statements) = when.statements() {
                self.visit(&statements.as_node());
                if !control_flow::diverges(&statements.as_node()) {
                    surviving.push(self.flow.local_callables.clone());
                }
            } else {
                surviving.push(self.flow.local_callables.clone());
            }
        }
        self.flow.local_callables = before.clone();
        if let Some(else_clause) = node.else_clause() {
            if let Some(statements) = else_clause.statements() {
                self.visit(&statements.as_node());
                if !control_flow::diverges(&statements.as_node()) {
                    surviving.push(self.flow.local_callables.clone());
                }
            } else {
                surviving.push(self.flow.local_callables.clone());
            }
        } else {
            surviving.push(before.clone());
        }
        self.flow.local_callables = surviving
            .into_iter()
            .reduce(Self::merge_local_callables)
            .unwrap_or(before);
    }

    fn visit_if_node(&mut self, node: &IfNode<'_>) {
        self.visit(&node.predicate());
        let before = self.flow.local_callables.clone();

        self.flow.local_callables = before.clone();
        if let Some(statements) = node.statements() {
            self.visit(&statements.as_node());
        }
        let then_callables = self.flow.local_callables.clone();
        let then_diverges = node
            .statements()
            .is_some_and(|statements| control_flow::diverges(&statements.as_node()));

        self.flow.local_callables = before.clone();
        if let Some(subsequent) = node.subsequent() {
            self.visit(&subsequent);
        }
        let else_callables = self.flow.local_callables.clone();
        let else_diverges = node
            .subsequent()
            .is_some_and(|subsequent| control_flow::diverges(&subsequent));

        self.flow.local_callables = match (then_diverges, else_diverges) {
            (true, true) => before,
            (true, false) => else_callables,
            (false, true) => then_callables,
            (false, false) => Self::merge_local_callables(then_callables, else_callables),
        };
    }

    fn visit_unless_node(&mut self, node: &UnlessNode<'_>) {
        self.visit(&node.predicate());
        let before = self.flow.local_callables.clone();

        self.flow.local_callables = before.clone();
        if let Some(statements) = node.statements() {
            self.visit(&statements.as_node());
        }
        let then_callables = self.flow.local_callables.clone();
        let then_diverges = node
            .statements()
            .is_some_and(|statements| control_flow::diverges(&statements.as_node()));

        self.flow.local_callables = before.clone();
        if let Some(else_clause) = node.else_clause() {
            self.visit(&else_clause.as_node());
        }
        let else_callables = self.flow.local_callables.clone();
        let else_diverges = node
            .else_clause()
            .is_some_and(|else_clause| control_flow::diverges(&else_clause.as_node()));

        self.flow.local_callables = match (then_diverges, else_diverges) {
            (true, true) => before,
            (true, false) => else_callables,
            (false, true) => then_callables,
            (false, false) => Self::merge_local_callables(then_callables, else_callables),
        };
    }

    fn visit_program_node(&mut self, node: &ProgramNode<'_>) {
        // Install exact root-scope flow evidence before the ordinary semantic
        // traversal consumes local receivers. Method bodies do the same from
        // `process_def_node_entry`; without the corresponding program pass,
        // a top-level alias mutation or escape would be analyzed through the
        // older assignment-only view and could publish stale shape fields.
        if self.options.record_local_read_unknown_reasons {
            let mut tracker = TypeTracker::new()
                .with_analysis_engine(self.semantics.engine.clone())
                .with_analysis_query_cache(self.semantics.query_cache.clone())
                .with_local_read_types();
            tracker.track_program(node);
            self.install_local_read_types(tracker.take_local_read_types());
        }
        visit_program_node(self, node);
        assert!(
            self.flow.active_writes.is_empty(),
            "INVARIANT VIOLATED: nonlocal write traversal remained active after the program walk. This is a bug because every variable-write entry must have a matching exit. Fix: balance the FactCollector write callbacks for every Prism write-node form."
        );
        self.finalize_all_method_return_equations();
    }

    fn visit_case_match_node(&mut self, node: &CaseMatchNode) {
        let predicate = node.predicate();
        if let Some(predicate) = &predicate {
            self.visit(predicate);
        }

        for condition in node.conditions().iter() {
            let Some(in_node) = condition.as_in_node() else {
                self.visit(&condition);
                continue;
            };

            let pattern = in_node.pattern();
            let captures = predicate
                .as_ref()
                .map(|value| self.pattern_capture_types_for_value(&pattern, value))
                .unwrap_or_default();
            self.flow.pattern_captures.push(captures);
            self.visit(&pattern);
            if let Some(statements) = in_node.statements() {
                self.visit(&statements.as_node());
            }
            self.flow.pattern_captures.pop().expect(
                "INVARIANT VIOLATED: pattern capture type stack underflow after case/in branch. \
                 This is a bug because each pushed pattern capture frame must be popped exactly once. \
                 Fix: keep FactCollector::visit_case_match_node branch traversal balanced.",
            );
        }

        if let Some(else_clause) = node.else_clause() {
            self.visit(&else_clause.as_node());
        }
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        self.collect_nil_call_candidate(node);
        self.invalidate_escaped_callables_in_call(node);
        self.process_call_node_entry(node);
        let mut prepared_higher_order = None;
        let extension_context = self.extensions.pending_block.take();
        if let Some(context) = extension_context {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let block = node.block().expect(
                "INVARIANT VIOLATED: extension execution context was applied to a call without a block. This is a bug because the host must validate the context against the current AST call. Fix: reject execution contexts whose call has no block.",
            );
            assert_eq!(
                context.block_range,
                self.direct_range(&block.location()),
                "INVARIANT VIOLATED: extension execution context block range differs from the traversed block. This is a bug because a guest must not redirect execution semantics to unrelated source. Fix: validate the exact call and block ranges at the extension boundary."
            );
            self.scope_tracker.push_block_execution_context(
                context.implicit_receiver,
                context.implicit_receiver_kind,
                context.method_definition_owner,
                context.method_definition_kind,
            );
            self.flow.block_parameters.push(Vec::new());
            self.visit(&block);
            self.flow.block_parameters.pop().expect(
                "INVARIANT VIOLATED: block parameter type stack underflow after extension execution context. This is a bug because each pushed block type frame must be popped exactly once. Fix: keep FactCollector::visit_call_node extension traversal balanced.",
            );
            self.scope_tracker.pop_execution_context();
        } else if let Some((
            implicit_namespace,
            implicit_kind,
            definition_namespace,
            definition_kind,
        )) = self.static_dynamic_definition_block_context(node)
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let block = node.block().expect(
                "INVARIANT VIOLATED: dynamic-definition block context lost its block. This is a bug because static_dynamic_definition_block_context required the same immutable Prism call to have a block. Fix: keep call traversal and context matching atomic.",
            );
            self.scope_tracker.push_block_execution_context(
                implicit_namespace.clone(),
                implicit_kind,
                definition_namespace,
                definition_kind,
            );
            self.push_direct_dynamic_definition_block_return_type(node, implicit_namespace);
            self.flow.block_parameters.push(Vec::new());
            self.visit(&block);
            self.flow.block_parameters.pop().expect(
                "INVARIANT VIOLATED: block parameter type stack underflow after dynamic-definition block. This is a bug because every pushed block type frame must be popped exactly once. Fix: keep FactCollector::visit_call_node dynamic-definition traversal balanced.",
            );
            self.scope_tracker.pop_execution_context();
        } else if let Some((eval_namespace, implicit_kind, definition_kind)) =
            self.static_eval_block_context(node)
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_block_execution_context(
                    eval_namespace.clone(),
                    implicit_kind,
                    eval_namespace,
                    definition_kind,
                );
                self.flow.block_parameters.push(Vec::new());
                self.visit(&block);
                self.flow.block_parameters.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after static eval block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
                self.scope_tracker.pop_execution_context();
            }
        } else if let Some(class_methods_namespace) =
            self.concern_class_methods_block_namespace(node)
        {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_ns_scopes(class_methods_namespace);
                self.flow.block_parameters.push(Vec::new());
                self.visit(&block);
                self.flow.block_parameters.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after Concern class_methods block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
                self.scope_tracker.pop_ns_scope();
            }
        } else {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                let (block_param_types, prepared) = if self.options.infer_call_outcomes {
                    self.infer_block_param_types_for_call(node)
                } else {
                    (Vec::new(), None)
                };
                prepared_higher_order = prepared;
                self.flow.block_parameters.push(block_param_types);
                let framework_instance_block =
                    crate::indexer::is_framework_instance_block_call_name(node.name().as_slice())
                        && node.receiver().is_none();
                if framework_instance_block {
                    self.scope_tracker
                        .push_scope_kind(crate::indexer::LocalScopeKind::FrameworkInstanceBlock);
                }
                self.visit(&block);
                if framework_instance_block {
                    self.scope_tracker.pop_scope_kind();
                }
                self.flow.block_parameters.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after call block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
            }
        }
        self.process_nested_receiver_call_reference_candidate(node);
        if self.options.infer_call_outcomes {
            self.record_call_expression_type(node, prepared_higher_order);
        }
        self.record_current_method_forwarded_yield_types(node);
        self.process_call_node_exit(node);
    }

    fn visit_yield_node(&mut self, node: &YieldNode) {
        self.record_current_method_yield_types(node);
        visit_yield_node(self, node);
    }

    fn visit_forwarding_super_node(&mut self, node: &ForwardingSuperNode) {
        self.process_forwarding_super_node_entry(node);
        visit_forwarding_super_node(self, node);
    }

    fn visit_super_node(&mut self, node: &SuperNode) {
        self.process_super_node_entry(node);
        visit_super_node(self, node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        self.process_constant_read_node_entry(node);
        visit_constant_read_node(self, node);
        self.process_constant_read_node_exit(node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        self.process_constant_path_node_entry(node);
        visit_constant_path_node(self, node);
        self.process_constant_path_node_exit(node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_variable_read_node_entry(node);
        visit_local_variable_read_node(self, node);
        self.process_local_variable_read_node_exit(node);
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        if !self.process_module_node_entry(node) {
            visit_module_node(self, node);
            return;
        }
        visit_module_node(self, node);
        let namespace = self.scope_tracker.get_ns_stack();
        self.finalize_method_return_equations_for_namespace(&namespace);
        self.process_module_node_exit(node);
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        if !self.process_class_node_entry(node) {
            visit_class_node(self, node);
            return;
        }
        visit_class_node(self, node);
        let namespace = self.scope_tracker.get_ns_stack();
        self.finalize_method_return_equations_for_namespace(&namespace);
        self.process_class_node_exit(node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        let namespace = self.scope_tracker.get_ns_stack();
        self.finalize_method_return_equations_for_namespace(&namespace);
        self.process_singleton_class_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        if !self.process_def_node_entry(node) {
            visit_def_node(self, node);
            return;
        }
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_alias_method_node(&mut self, node: &AliasMethodNode) {
        self.process_alias_method_node_entry(node);
        visit_alias_method_node(self, node);
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.process_block_node_entry(node);
        visit_block_node(self, node);
        self.process_block_node_exit(node);
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode) {
        self.process_constant_write_node_entry(node);
        visit_constant_write_node(self, node);
        self.process_constant_write_node_exit(node);
    }

    fn visit_constant_or_write_node(&mut self, node: &ConstantOrWriteNode) {
        self.process_constant_or_write_node_entry(node);
        visit_constant_or_write_node(self, node);
        self.process_constant_or_write_node_exit(node);
    }

    fn visit_constant_and_write_node(&mut self, node: &ConstantAndWriteNode) {
        self.process_constant_and_write_node_entry(node);
        visit_constant_and_write_node(self, node);
        self.process_constant_and_write_node_exit(node);
    }

    fn visit_constant_operator_write_node(&mut self, node: &ConstantOperatorWriteNode) {
        self.process_constant_operator_write_node_entry(node);
        visit_constant_operator_write_node(self, node);
        self.process_constant_operator_write_node_exit(node);
    }

    fn visit_constant_target_node(&mut self, node: &ConstantTargetNode) {
        self.process_constant_target_node_entry(node);
        visit_constant_target_node(self, node);
        self.process_constant_target_node_exit(node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode) {
        self.process_constant_path_write_node_entry(node);
        visit_constant_path_write_node(self, node);
        self.process_constant_path_write_node_exit(node);
    }

    fn visit_constant_path_or_write_node(&mut self, node: &ConstantPathOrWriteNode) {
        self.process_constant_path_or_write_node_entry(node);
        visit_constant_path_or_write_node(self, node);
        self.process_constant_path_or_write_node_exit(node);
    }

    fn visit_constant_path_and_write_node(&mut self, node: &ConstantPathAndWriteNode) {
        self.process_constant_path_and_write_node_entry(node);
        visit_constant_path_and_write_node(self, node);
        self.process_constant_path_and_write_node_exit(node);
    }

    fn visit_constant_path_operator_write_node(&mut self, node: &ConstantPathOperatorWriteNode) {
        self.process_constant_path_operator_write_node_entry(node);
        visit_constant_path_operator_write_node(self, node);
        self.process_constant_path_operator_write_node_exit(node);
    }

    fn visit_multi_write_node(&mut self, node: &MultiWriteNode) {
        // Visit the RHS first so expression/method-return facts exist, then
        // push positional element types for ConstantTarget consumers.
        self.visit(&node.value());
        let element_types = self.multi_write_element_types(&node.value());
        self.flow.assignment_elements.push(element_types);
        for target in node.lefts().iter() {
            self.visit(&target);
        }
        if let Some(rest) = node.rest() {
            self.visit(&rest);
        }
        for target in node.rights().iter() {
            self.visit(&target);
        }
        self.flow.assignment_elements.pop().expect(
            "INVARIANT VIOLATED: multi-write LHS type stack underflow. \
             This is a bug because each MultiWriteNode push must be balanced by one pop. \
             Fix: keep FactCollector::visit_multi_write_node stack frames paired.",
        );
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write_node_entry(node);
        visit_local_variable_write_node(self, node);
        self.process_local_variable_write_node_exit(node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_target_node_entry(node);
        visit_local_variable_target_node(self, node);
        self.process_local_variable_target_node_exit(node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_or_write_node_entry(node);
        visit_local_variable_or_write_node(self, node);
        self.process_local_variable_or_write_node_exit(node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode) {
        self.process_local_variable_and_write_node_entry(node);
        visit_local_variable_and_write_node(self, node);
        self.process_local_variable_and_write_node_exit(node);
    }

    fn visit_local_variable_operator_write_node(&mut self, node: &LocalVariableOperatorWriteNode) {
        self.process_local_variable_operator_write_node_entry(node);
        visit_local_variable_operator_write_node(self, node);
        self.process_local_variable_operator_write_node_exit(node);
    }

    fn visit_parameters_node(&mut self, node: &ruby_prism::ParametersNode<'_>) {
        self.process_parameters_node_entry(node);
        visit_parameters_node(self, node);
        self.process_parameters_node_exit(node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write_node_entry(node);
        visit_class_variable_write_node(self, node);
        self.process_class_variable_write_node_exit(node);
    }

    fn visit_class_variable_read_node(&mut self, node: &ClassVariableReadNode) {
        self.process_class_variable_read_node_entry(node);
        visit_class_variable_read_node(self, node);
    }

    fn visit_class_variable_target_node(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_target_node_entry(node);
        visit_class_variable_target_node(self, node);
        self.process_class_variable_target_node_exit(node);
    }

    fn visit_class_variable_or_write_node(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_or_write_node_entry(node);
        visit_class_variable_or_write_node(self, node);
        self.process_class_variable_or_write_node_exit(node);
    }

    fn visit_class_variable_and_write_node(&mut self, node: &ClassVariableAndWriteNode) {
        self.process_class_variable_and_write_node_entry(node);
        visit_class_variable_and_write_node(self, node);
        self.process_class_variable_and_write_node_exit(node);
    }

    fn visit_class_variable_operator_write_node(&mut self, node: &ClassVariableOperatorWriteNode) {
        self.process_class_variable_operator_write_node_entry(node);
        visit_class_variable_operator_write_node(self, node);
        self.process_class_variable_operator_write_node_exit(node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write_node_entry(node);
        visit_instance_variable_write_node(self, node);
        self.process_instance_variable_write_node_exit(node);
    }

    fn visit_instance_variable_read_node(&mut self, node: &InstanceVariableReadNode) {
        self.process_instance_variable_read_node_entry(node);
        visit_instance_variable_read_node(self, node);
    }

    fn visit_instance_variable_target_node(&mut self, node: &InstanceVariableTargetNode) {
        self.process_instance_variable_target_node_entry(node);
        visit_instance_variable_target_node(self, node);
        self.process_instance_variable_target_node_exit(node);
    }

    fn visit_instance_variable_or_write_node(&mut self, node: &InstanceVariableOrWriteNode) {
        self.process_instance_variable_or_write_node_entry(node);
        visit_instance_variable_or_write_node(self, node);
        self.process_instance_variable_or_write_node_exit(node);
    }

    fn visit_instance_variable_and_write_node(&mut self, node: &InstanceVariableAndWriteNode) {
        self.process_instance_variable_and_write_node_entry(node);
        visit_instance_variable_and_write_node(self, node);
        self.process_instance_variable_and_write_node_exit(node);
    }

    fn visit_instance_variable_operator_write_node(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.process_instance_variable_operator_write_node_entry(node);
        visit_instance_variable_operator_write_node(self, node);
        self.process_instance_variable_operator_write_node_exit(node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write_node_entry(node);
        visit_global_variable_write_node(self, node);
        self.process_global_variable_write_node_exit(node);
    }

    fn visit_global_variable_read_node(&mut self, node: &GlobalVariableReadNode) {
        self.process_global_variable_read_node_entry(node);
        visit_global_variable_read_node(self, node);
    }

    fn visit_global_variable_target_node(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_target_node_entry(node);
        visit_global_variable_target_node(self, node);
        self.process_global_variable_target_node_exit(node);
    }

    fn visit_global_variable_or_write_node(&mut self, node: &GlobalVariableOrWriteNode) {
        self.process_global_variable_or_write_node_entry(node);
        visit_global_variable_or_write_node(self, node);
        self.process_global_variable_or_write_node_exit(node);
    }

    fn visit_global_variable_and_write_node(&mut self, node: &GlobalVariableAndWriteNode) {
        self.process_global_variable_and_write_node_entry(node);
        visit_global_variable_and_write_node(self, node);
        self.process_global_variable_and_write_node_exit(node);
    }

    fn visit_global_variable_operator_write_node(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_operator_write_node_entry(node);
        visit_global_variable_operator_write_node(self, node);
        self.process_global_variable_operator_write_node_exit(node);
    }
}
