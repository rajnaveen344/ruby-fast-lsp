use ruby_prism::ParametersNode;

use crate::indexer::Identifier;

use crate::indexer::identifiers::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor<'_> {
    pub fn process_parameters_node_entry(&mut self, node: &ParametersNode) {
        if self.is_result_set() || !self.is_position_in_location(&node.location()) {
            return;
        }

        for required in node.requireds().iter() {
            if let Some(param) = required.as_required_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    self.set_parameter_definition(param.name().as_slice());
                }
            }
        }

        for optional in node.optionals().iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                // Match only the name; a cursor in the default value belongs to
                // that expression's own visitors.
                if self.is_position_in_location(&param.name_loc()) {
                    self.set_parameter_definition(param.name().as_slice());
                }
            }
        }

        if let Some(param) = node.rest().and_then(|rest| rest.as_rest_parameter_node()) {
            if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                if self.is_position_in_location(&name_loc) {
                    self.set_parameter_definition(name.as_slice());
                }
            }
        }

        for post in node.posts().iter() {
            if let Some(param) = post.as_required_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    self.set_parameter_definition(param.name().as_slice());
                }
            }
        }

        // Keyword name locations include the trailing `:`; match only the name.
        for keyword in node.keywords().iter() {
            let (name, name_start) =
                if let Some(param) = keyword.as_required_keyword_parameter_node() {
                    (param.name(), param.name_loc().start_offset())
                } else if let Some(param) = keyword.as_optional_keyword_parameter_node() {
                    (param.name(), param.name_loc().start_offset())
                } else {
                    continue;
                };
            let name = name.as_slice();
            if self.is_position_in_offsets(name_start, name_start + name.len()) {
                self.set_parameter_definition(name);
            }
        }

        if let Some(param) = node
            .keyword_rest()
            .and_then(|rest| rest.as_keyword_rest_parameter_node())
        {
            if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                if self.is_position_in_location(&name_loc) {
                    self.set_parameter_definition(name.as_slice());
                }
            }
        }

        if let Some(param) = node.block() {
            if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                if self.is_position_in_location(&name_loc) {
                    self.set_parameter_definition(name.as_slice());
                }
            }
        }
    }

    fn set_parameter_definition(&mut self, name: &[u8]) {
        self.set_result(
            Some(Identifier::RubyLocalVariable {
                namespace: self.scope_tracker.get_ns_stack(),
                name: String::from_utf8_lossy(name).to_string(),
            }),
            Some(IdentifierType::LVarDef),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
    }

    pub fn process_parameters_node_exit(&mut self, _node: &ParametersNode) {
        // No cleanup needed for parameters
    }
}
