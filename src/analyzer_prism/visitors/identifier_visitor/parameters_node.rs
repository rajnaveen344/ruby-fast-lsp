use ruby_prism::ParametersNode;

use crate::analyzer_prism::Identifier;

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_parameters_node_entry(&mut self, node: &ParametersNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        // Required parameters
        let requireds = node.requireds();
        for required in requireds.iter() {
            if let Some(param) = required.as_required_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                    let scope_id = self
                        .scope_tracker
                        .current_lv_scope()
                        .map(|s| s.scope_id())
                        .unwrap_or(0);
                    self.set_result(
                        Some(Identifier::RubyLocalVariable {
                            namespace: self.scope_tracker.get_ns_stack(),
                            name: param_name,
                            scope: scope_id,
                        }),
                        Some(IdentifierType::LVarDef),
                        self.scope_tracker.get_ns_stack(),
                        Some(scope_id),
                    );
                }
            }
        }

        // Optional parameters
        let optionals = node.optionals();
        for optional in optionals.iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                // Check if cursor is on the parameter name, not on the default value
                // For optional parameters, we need to check the name location specifically
                // to avoid matching constants in the default value expression
                let value = param.value();
                let value_location = value.location();

                // If cursor is in the default value, don't match the parameter name
                // This allows constants in the default value to be processed by their own visitors
                if !self.is_position_in_location(&value_location) {
                    // Cursor is not in the default value, check if it's in the parameter name
                    if self.is_position_in_location(&param.location()) {
                        let param_name =
                            String::from_utf8_lossy(param.name().as_slice()).to_string();
                        let scope_id = self
                            .scope_tracker
                            .current_lv_scope()
                            .map(|s| s.scope_id())
                            .unwrap_or(0);
                        self.set_result(
                            Some(Identifier::RubyLocalVariable {
                                namespace: self.scope_tracker.get_ns_stack(),
                                name: param_name,
                                scope: scope_id,
                            }),
                            Some(IdentifierType::LVarDef),
                            self.scope_tracker.get_ns_stack(),
                            Some(scope_id),
                        );
                    }
                }
            }
        }

        // Rest parameters
        if let Some(rest) = node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let Some(name) = param.name() {
                    if self.is_position_in_location(&param.location()) {
                        let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                        let scope_id = self
                            .scope_tracker
                            .current_lv_scope()
                            .map(|s| s.scope_id())
                            .unwrap_or(0);
                        self.set_result(
                            Some(Identifier::RubyLocalVariable {
                                namespace: self.scope_tracker.get_ns_stack(),
                                name: param_name,
                                scope: scope_id,
                            }),
                            Some(IdentifierType::LVarDef),
                            self.scope_tracker.get_ns_stack(),
                            Some(scope_id),
                        );
                    }
                }
            }
        }

        // Post parameters
        for post in node.posts().iter() {
            if let Some(param) = post.as_required_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                    let scope_id = self
                        .scope_tracker
                        .current_lv_scope()
                        .map(|s| s.scope_id())
                        .unwrap_or(0);
                    self.set_result(
                        Some(Identifier::RubyLocalVariable {
                            namespace: self.scope_tracker.get_ns_stack(),
                            name: param_name,
                            scope: scope_id,
                        }),
                        Some(IdentifierType::LVarDef),
                        self.scope_tracker.get_ns_stack(),
                        Some(scope_id),
                    );
                }
            }
        }

        // TODO: keywords, keyword_rest, block
    }

    pub fn process_parameters_node_exit(&mut self, _node: &ParametersNode) {
        // No cleanup needed for parameters
    }
}
