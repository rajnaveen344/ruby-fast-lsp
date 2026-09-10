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
        let flow_delta = flow_type
            .as_ref()
            .is_some_and(|ruby_type| assignment_type.as_ref() != Some(ruby_type));
        let reaching_type = flow_type.or(assignment_type);
        // Cold-indexed documents do not retain the collector's VariableScopes.
        // Keep block-owned reads as well as flow deltas so opening unchanged
        // content preserves parameter types without guessing lexical scope.
        let block_owned = owner_scope_id.is_some_and(|scope_id| {
            self.document.variable_scopes().scope_kind(scope_id)
                == Some(crate::LocalScopeKind::Block)
        });
        if let Some(ruby_type) = reaching_type
            .as_ref()
            .filter(|ruby_type| **ruby_type != RubyType::Unknown && (block_owned || flow_delta))
        {
            self.local_read_types.push((range, ruby_type.clone()));
        }
        let unknown_reason = match reaching_type {
            None => Some(UnknownReason::NoReachingAssignment),
            Some(RubyType::Unknown) => Some(UnknownReason::UnresolvedAssignmentValue),
            Some(
                RubyType::Class(_)
                | RubyType::Module(_)
                | RubyType::ClassReference(_)
                | RubyType::ModuleReference(_)
                | RubyType::Literal(_)
                | RubyType::Array(_)
                | RubyType::Hash(_, _)
                | RubyType::Shape(_)
                | RubyType::Union(_),
            ) => None,
        };
        if let Some(reason) = unknown_reason {
            // TypeTracker may already have installed a more precise flow
            // reason (for example mutable_shape_invalidated) before the
            // ordinary visitor reaches this exact read. Keep that proof
            // barrier instead of adding the generic Unknown projection as a
            // conflicting second result.
            self.expression_unknown_reasons
                .entry(range)
                .or_insert(reason);
        }
    }

    pub fn process_local_variable_read_node_exit(&mut self, _node: &LocalVariableReadNode) {}
}
