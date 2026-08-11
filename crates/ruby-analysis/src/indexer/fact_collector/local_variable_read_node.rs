use crate::core::UnknownReason;
use crate::inference::RubyType;
use ruby_prism::LocalVariableReadNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_local_variable_read_node_entry(&mut self, node: &LocalVariableReadNode) {
        if !self.include_local_vars {
            return;
        }

        let variable_name = crate::utf8_str(node.name().as_slice());
        let range = self.document.prism_location_to_text_range(&node.location());

        let owner_scope_id = self
            .document
            .variable_scopes_mut()
            .reference_variable(variable_name, range)
            .map(|(scope_id, _variable_index, _captured)| scope_id);
        if !self.record_local_read_unknown_reasons {
            return;
        }
        let (flow_type, assignment_type) = owner_scope_id
            .map(|scope_id| {
                let (flow_type, assignment_type) =
                    self.document.variable_scopes().get_read_types_at_position(
                        variable_name,
                        scope_id,
                        self.document.analysis_file_id(),
                        range.start_byte,
                    );
                (flow_type.cloned(), assignment_type.cloned())
            })
            .unwrap_or((None, None));
        if let Some(flow_type) = flow_type.as_ref().filter(|ruby_type| {
            **ruby_type != RubyType::Unknown && assignment_type.as_ref() != Some(*ruby_type)
        }) {
            self.local_read_types.push((range, (*flow_type).clone()));
        }
        let reaching_type = flow_type.or(assignment_type);
        let unknown_reason = match reaching_type {
            None => Some(UnknownReason::NoReachingAssignment),
            Some(RubyType::Unknown) => Some(UnknownReason::UnresolvedAssignmentValue),
            Some(
                RubyType::Class(_)
                | RubyType::Module(_)
                | RubyType::ClassReference(_)
                | RubyType::ModuleReference(_)
                | RubyType::Array(_)
                | RubyType::Hash(_, _)
                | RubyType::Union(_),
            ) => None,
        };
        if let Some(reason) = unknown_reason {
            self.expression_unknown_reasons.push((range, reason));
        }
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {}
}
