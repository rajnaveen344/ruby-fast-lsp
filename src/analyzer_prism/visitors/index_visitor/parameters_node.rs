use log::error;
use ruby_prism::ParametersNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

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

        // Get current scope id for the parameter
        let current_scope = match self.scope_tracker.current_lv_scope() {
            Some(scope) => scope.scope_id(),
            None => {
                error!(
                    "No current local variable scope available for parameter: {}",
                    param_name
                );
                return;
            }
        };

        // Create a fully qualified name for the parameter (local variable)
        let fqn =
            FullyQualifiedName::local_variable(param_name.to_string(), current_scope).unwrap();

        // Create an entry with EntryKind::LocalVariable
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.document.prism_location_to_lsp_location(&location))
            .kind(EntryKind::new_local_variable(
                param_name.to_string(),
                current_scope,
                RubyType::Unknown,
            ))
            .build();

        // Add the entry to RubyDocument only (NOT global index)
        // Parameters are LocalVariables - file-local data should not bloat the global index
        if let Ok(entry) = entry {
            self.document
                .add_local_var_entry(current_scope, entry.clone());
        } else {
            error!("Error creating entry for parameter: {}", param_name);
        }
    }

    pub fn process_parameters_node_exit(&mut self, _node: &ParametersNode) {
        // No-op for now
    }
}
