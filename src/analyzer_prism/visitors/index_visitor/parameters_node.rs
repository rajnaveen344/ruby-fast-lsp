use log::error;
use ruby_prism::ParametersNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    /// Ruby supports 7 types of method parameters
    /// 1. Required parameters
    /// 2. Optional parameters
    /// 3. Rest parameters
    /// 4. Post parameters
    /// 5. Keyword parameters
    /// 6. Keyword rest parameters
    /// 7. Block parameter
    pub fn process_parameters_node_entry(&mut self, node: &ParametersNode) {
        // Process required parameters
        let requireds = node.requireds();
        for required in requireds.iter() {
            if let Some(param) = required.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.add_parameter_to_index(&param_name, param.location());
            }
        }

        // Process optional parameters
        let optionals = node.optionals();
        for optional in optionals.iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.add_parameter_to_index(&param_name, param.location());
            }
        }

        // Process rest parameter
        if let Some(rest) = node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let Some(name) = param.name() {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    self.add_parameter_to_index(&param_name, param.location());
                }
            }
        }

        // Process post parameters
        let posts = node.posts();
        for post in posts.iter() {
            if let Some(param) = post.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.add_parameter_to_index(&param_name, param.location());
            }
        }

        // TODO: keywords, keyword_rest, block
    }

    // Helper method to add a parameter to the index
    fn add_parameter_to_index(&mut self, param_name: &str, location: ruby_prism::Location) {
        let var = RubyVariable::new(
            param_name,
            RubyVariableType::Local(self.scope_tracker.get_lv_stack().to_vec()),
        );

        match var {
            Ok(variable) => {
                // Create a fully qualified name for the variable
                let fqn = FullyQualifiedName::variable(variable.clone());

                // Create an entry with EntryKind::Variable
                let entry = EntryBuilder::new()
                    .fqn(fqn)
                    .location(self.document.prism_location_to_lsp_location(&location))
                    .kind(EntryKind::new_variable(variable.clone(), None))
                    .build();

                // Add the entry to the index
                if let Ok(entry) = entry {
                    let mut index = self.index.lock();
                    index.add_entry(entry.clone());

                    // Safely get the current scope before adding local variable entry
                    if let Some(current_scope) = self.scope_tracker.current_lv_scope() {
                        self.document
                            .add_local_var_entry(current_scope.scope_id(), entry.clone());
                    } else {
                        error!(
                            "No current local variable scope available for parameter: {}",
                            param_name
                        );
                    }
                } else {
                    error!("Error creating entry for parameter: {}", param_name);
                }
            }
            Err(err) => {
                error!("Invalid parameter name '{}': {}", param_name, err);
            }
        }
    }

    pub fn process_parameters_node_exit(&mut self, _node: &ParametersNode) {
        // No-op for now
    }
}
