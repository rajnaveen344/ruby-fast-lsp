use log::error;
use ruby_prism::ParametersNode;

use crate::core::{RubyType, TextRange, TypeResolution, TypeSubject};

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    /// Ruby supports 7 types of method parameters
    /// 1. Required parameters
    /// 2. Optional parameters
    /// 3. Rest parameters
    /// 4. Post parameters
    /// 5. Keyword parameters
    /// 6. Keyword rest parameters
    /// 7. Block parameter
    pub(in crate::indexer::fact_collector) fn process_parameters_node_entry(
        &mut self,
        node: &ParametersNode,
    ) {
        let mut positional_index = 0usize;

        // Process required parameters
        let requireds = node.requireds();
        for required in requireds.iter() {
            if let Some(param) = required.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.add_parameter_to_index(&param_name, &param.location());
                self.assign_current_block_parameter_type(
                    &param_name,
                    &param.location(),
                    positional_index,
                );
                positional_index += 1;
            }
        }

        // Process optional parameters
        let optionals = node.optionals();
        for optional in optionals.iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                let name_loc = param.name_loc();
                self.add_parameter_to_index(&param_name, &name_loc);
                self.assign_current_block_parameter_type(&param_name, &name_loc, positional_index);
                positional_index += 1;
            }
        }

        // Process rest parameter
        if let Some(rest) = node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    self.add_parameter_to_index(&param_name, &name_loc);
                    self.assign_current_block_parameter_type(
                        &param_name,
                        &name_loc,
                        positional_index,
                    );
                    positional_index += 1;
                }
            }
        }

        // Process post parameters
        let posts = node.posts();
        for post in posts.iter() {
            if let Some(param) = post.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                self.add_parameter_to_index(&param_name, &param.location());
                self.assign_current_block_parameter_type(
                    &param_name,
                    &param.location(),
                    positional_index,
                );
                positional_index += 1;
            }
        }

        // Keyword, keyword-rest, and block parameters are not positional, so
        // block argument types are not assigned to them.
        for keyword in node.keywords().iter() {
            let (name, name_loc) = if let Some(param) = keyword.as_required_keyword_parameter_node()
            {
                (param.name(), param.name_loc())
            } else if let Some(param) = keyword.as_optional_keyword_parameter_node() {
                (param.name(), param.name_loc())
            } else {
                continue;
            };
            // The keyword name location includes the trailing `:`.
            let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
            let mut name_range = self.document.prism_location_to_text_range(&name_loc);
            name_range.end_byte = name_range.start_byte + name.as_slice().len() as u32;
            self.add_parameter_range_to_index(&param_name, name_range);
        }

        if let Some(param) = node
            .keyword_rest()
            .and_then(|rest| rest.as_keyword_rest_parameter_node())
        {
            if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                self.add_parameter_to_index(&param_name, &name_loc);
            }
        }

        if let Some(param) = node.block() {
            if let (Some(name), Some(name_loc)) = (param.name(), param.name_loc()) {
                let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                self.add_parameter_to_index(&param_name, &name_loc);
            }
        }
    }

    fn add_parameter_to_index(&mut self, param_name: &str, location: &ruby_prism::Location) {
        let text_range = self.document.prism_location_to_text_range(location);
        self.add_parameter_range_to_index(param_name, text_range);
    }

    // Helper method to add a parameter to collected facts/scopes.
    fn add_parameter_range_to_index(&mut self, param_name: &str, text_range: TextRange) {
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

        let parameter_type = self
            .scope_tracker
            .current_method_fqn()
            .map(|method| TypeSubject::Parameter {
                method: method.clone(),
                name: param_name.into(),
            })
            .map(|subject| {
                self.facts.flow_types.type_at(
                    &subject,
                    self.document.analysis_file_id(),
                    text_range.start_byte,
                )
            })
            .and_then(|resolution| match resolution {
                TypeResolution::Resolved(fact) => Some(fact.ruby_type),
                TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
            })
            .unwrap_or(RubyType::Unknown);
        self.document
            .variable_scopes_mut()
            .define_variable(param_name, text_range);

        // Dual-write the explicit parameter contract when one exists. An
        // unannotated parameter remains Unknown until some other proof narrows
        // or replaces it.
        if let Some(current_scope_id) = self.document.variable_scopes().current_scope() {
            self.document.variable_scopes_mut().add_type_assignment(
                current_scope_id,
                param_name,
                text_range,
                parameter_type,
            );
        }
    }

    pub(in crate::indexer::fact_collector) fn process_parameters_node_exit(
        &mut self,
        _node: &ParametersNode,
    ) {
        // No-op for now
    }
}
