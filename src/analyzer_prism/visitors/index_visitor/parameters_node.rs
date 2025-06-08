use log::{debug, error, info};
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
        debug!("Visiting parameters node");

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
        debug!("Adding parameter: {}", param_name);

        let var = RubyVariable::new(
            param_name,
            RubyVariableType::Local(self.current_lv_scope_depth(), self.current_lv_scope_kind()),
        );

        info!("Adding local variable entry: {:?}", var.clone().unwrap());
        match var {
            Ok(variable) => {
                // Create a fully qualified name for the variable
                let fqn = FullyQualifiedName::variable(
                    self.uri.clone(),
                    self.namespace_stack.clone(),
                    self.current_method.clone(),
                    variable.clone(),
                );

                // Create an entry with EntryKind::Variable
                let entry = EntryBuilder::new()
                    .fqn(fqn)
                    .location(self.prism_loc_to_lsp_loc(location))
                    .kind(EntryKind::Variable {
                        name: variable.clone(),
                    })
                    .build();

                // Add the entry to the index
                if let Ok(entry) = entry {
                    let mut index = self.index.lock().unwrap();
                    index.add_entry(entry);
                    debug!("Added parameter entry: {:?}", variable);
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
