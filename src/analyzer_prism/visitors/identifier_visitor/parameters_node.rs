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
                    self.set_result(
                        Some(Identifier::RubyLocalVariable {
                            namespace: self.scope_tracker.get_ns_stack(),
                            name: param_name,
                            scope: self.scope_tracker.get_lv_stack().clone(),
                        }),
                        Some(IdentifierType::LVarDef),
                        self.scope_tracker.get_ns_stack(),
                        self.scope_tracker.get_lv_stack(),
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
                        self.set_result(
                            Some(Identifier::RubyLocalVariable {
                                namespace: self.scope_tracker.get_ns_stack(),
                                name: param_name,
                                scope: self.scope_tracker.get_lv_stack().clone(),
                            }),
                            Some(IdentifierType::LVarDef),
                            self.scope_tracker.get_ns_stack(),
                            self.scope_tracker.get_lv_stack(),
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
                        self.set_result(
                            Some(Identifier::RubyLocalVariable {
                                namespace: self.scope_tracker.get_ns_stack(),
                                name: param_name,
                                scope: self.scope_tracker.get_lv_stack().clone(),
                            }),
                            Some(IdentifierType::LVarDef),
                            self.scope_tracker.get_ns_stack(),
                            self.scope_tracker.get_lv_stack(),
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
                    self.set_result(
                        Some(Identifier::RubyLocalVariable {
                            namespace: self.scope_tracker.get_ns_stack(),
                            name: param_name,
                            scope: self.scope_tracker.get_lv_stack().clone(),
                        }),
                        Some(IdentifierType::LVarDef),
                        self.scope_tracker.get_ns_stack(),
                        self.scope_tracker.get_lv_stack(),
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
