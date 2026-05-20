use log::error;
use ruby_prism::ParametersNode;

use crate::inference::RubyType;

use super::FactCollector;

impl FactCollector {
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

    // Helper method to add a parameter to collected facts/scopes.
    fn add_parameter_to_index(&mut self, param_name: &str, location: ruby_prism::Location) {
        // Validate parameter name (should be a valid local variable name)
        if param_name.is_empty() {
            error!("Parameter name cannot be empty");
            return;
        }

        let mut chars = param_name.chars();
        let first = chars.next().unwrap();

        // Parameters must start with lowercase or underscore
        if !(first.is_lowercase() || first == '_') {
            error!(
                "Parameter name must start with lowercase or _: {}",
                param_name
            );
            return;
        }

        // Check for valid characters (alphanumeric and underscore)
        if !param_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            error!("Parameter name contains invalid characters: {}", param_name);
            return;
        }

        let text_range = self.document.prism_location_to_text_range(&location);
        self.document
            .variable_scopes_mut()
            .define_variable(param_name, text_range);

        // Dual-write: also store type in VariableScopes (Unknown for params initially)
        if let Some(current_scope_id) = self.document.variable_scopes().current_scope() {
            self.document.variable_scopes_mut().add_type_assignment(
                current_scope_id,
                param_name,
                text_range,
                RubyType::Unknown,
            );
        }
    }

    pub fn process_parameters_node_exit(&mut self, _node: &ParametersNode) {
        // No-op for now
    }
}
