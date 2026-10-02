use crate::core::{FullyQualifiedName, GraphEdgeKind, GraphNodeKind};
use crate::indexer::documents::scope_rules::{self, BlockExecution};
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
            .enter_scope(LVScopeKind::Block, body_range);
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
                    &param.name_loc(),
                    positional_index,
                );
                positional_index += 1;
            }
        }

        if let Some(rest) = params_node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    self.assign_current_block_parameter_type(
                        &param_name,
                        &name_loc,
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
    pub(in crate::indexer::fact_collector) fn eval_block(
        &self,
        node: &CallNode,
    ) -> Option<BlockExecution> {
        scope_rules::eval_block(node, &self.scope_tracker, |receiver| {
            self.resolve_constant_receiver_namespace(receiver)
        })
    }

    pub(in crate::indexer::fact_collector) fn dynamic_definition_block(
        &self,
        node: &CallNode,
    ) -> Option<BlockExecution> {
        scope_rules::dynamic_definition_block(node, &self.scope_tracker, |receiver| {
            self.resolve_constant_receiver_namespace(receiver)
        })
    }

    /// A Concern `class_methods` block: declares the `ClassMethods` module
    /// and extends its owner with it.
    pub(in crate::indexer::fact_collector) fn concern_class_methods_block(
        &mut self,
        node: &CallNode,
    ) -> Option<BlockExecution> {
        let execution = scope_rules::class_methods_block(
            node,
            scope_rules::implicit_singleton_namespace(&self.scope_tracker),
        )?;
        let target = execution.definition_namespace.clone();
        let owner = FullyQualifiedName::namespace(target[..target.len() - 1].to_vec());
        let range = self.direct_range(&node.location());
        self.direct_push_namespace_facts(
            FullyQualifiedName::namespace(target.clone()),
            GraphNodeKind::Module,
            range,
            range,
        );
        self.direct_push_edge(owner, &target, true, GraphEdgeKind::Extend, range);
        Some(execution)
    }
}
