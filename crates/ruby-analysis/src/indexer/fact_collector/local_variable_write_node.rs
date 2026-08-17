use crate::core::{FullyQualifiedName, SymbolKind, TypeFact, TypeProvenance, TypeSubject};
use log::error;
use ruby_prism::{
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, Location, Node,
};

use super::FactCollector;
use crate::inference::RubyType;

impl FactCollector {
    /// Process local variable write with type inference
    fn process_local_variable_write(
        &mut self,
        name: &[u8],
        name_loc: Location,
        value_node: Option<&Node>,
        explicit_type: Option<RubyType>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        if let Some(value) = value_node {
            self.invalidate_escaped_callables_in_value(value);
        }

        // Infer type from value if available
        let (inferred_type, inferred_unknown_reason) = if let Some(ty) = explicit_type {
            (ty, None)
        } else if let Some(value) = value_node {
            self.infer_assignment_type_from_value_with_reason(value)
        } else {
            (RubyType::Unknown, None)
        };
        let constant_dependency = value_node.and_then(|value| self.constant_type_dependency(value));

        // Validate the variable name
        if variable_name.is_empty() {
            error!("Local variable name cannot be empty");
            return;
        }

        let mut chars = variable_name.chars();
        let first = chars.next().unwrap();

        // Local variables must start with lowercase or underscore
        if !(first.is_lowercase() || first == '_') {
            error!(
                "Local variable name must start with lowercase or _: {}",
                variable_name
            );
            return;
        }

        // Check for valid characters (alphanumeric and underscore)
        if !variable_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            error!(
                "Local variable name contains invalid characters: {}",
                variable_name
            );
            return;
        }
        if let Some(value) = value_node {
            if let Some(return_type) = self.infer_known_proc_type(value) {
                self.bind_local_callable(variable_name.clone(), return_type);
            } else if let Some(alias) = value.as_local_variable_read_node() {
                let alias_name = String::from_utf8_lossy(alias.name().as_slice()).to_string();
                if let Some(callable) = self.proc_return_types_by_local.get(&alias_name).cloned() {
                    self.bind_local_callable(variable_name.clone(), callable);
                } else {
                    self.proc_return_types_by_local.remove(&variable_name);
                }
            } else {
                self.proc_return_types_by_local.remove(&variable_name);
            }
        }

        // Get location for both index entry and VariableScopes
        let location = self.document.prism_location_to_text_range(&name_loc);
        if let Some(reason) = inferred_unknown_reason {
            assert_eq!(
                inferred_type,
                RubyType::Unknown,
                "INVARIANT VIOLATED: local assignment retained shape construction reason `{}` with concrete type `{inferred_type}`. This is a bug because a proof failure and a concrete result cannot describe the same assignment. Fix: return exactly one state from assignment inference.",
                reason.code()
            );
            self.expression_unknown_reasons.push((location, reason));
        }
        if let Ok(fqn) = FullyQualifiedName::local_variable(variable_name.clone()) {
            self.direct_push_variable_symbol(fqn, SymbolKind::LocalVariable, &name_loc);
        }
        let root_subject = TypeSubject::Local {
            scope_id: 0,
            name: variable_name.clone(),
        };
        if inferred_type == RubyType::Unknown && constant_dependency.is_some() {
            self.direct_facts.types.push(TypeFact::new(
                root_subject.clone(),
                RubyType::Unknown,
                self.document.prism_location_to_text_range(&name_loc),
                TypeProvenance::Assignment,
            ));
        } else {
            self.direct_push_assignment_type(root_subject, inferred_type.clone(), &name_loc);
        }

        self.document
            .variable_scopes_mut()
            .define_variable(&variable_name, location);

        if let Some(current_scope_id) = self.document.variable_scopes().current_scope() {
            self.document.variable_scopes_mut().add_type_assignment(
                current_scope_id,
                &variable_name,
                location,
                inferred_type.clone(),
            );
            let scope_id = u32::try_from(current_scope_id).expect(
                "INVARIANT VIOLATED: local variable scope id exceeded u32. \
                 This is a bug because ruby-analysis::core TypeSubject::Local stores u32 scope ids. \
                 Fix: widen TypeSubject::Local scope_id before indexing more than u32::MAX scopes.",
            );
            let subject = TypeSubject::Local {
                scope_id,
                name: variable_name.clone(),
            };
            self.type_store.add(TypeFact::new(
                subject,
                inferred_type.clone(),
                self.document.prism_location_to_text_range(&name_loc),
                TypeProvenance::Assignment,
            ));
            if let Some(dependency) = constant_dependency {
                self.push_constant_local_assignment_equation(variable_name, location, dependency);
            }
        }
    }

    // LocalVariableWriteNode
    pub fn process_local_variable_write_node_entry(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    pub fn process_local_variable_write_node_exit(&mut self, _node: &LocalVariableWriteNode) {
        // No-op for now
    }

    // LocalVariableTargetNode
    pub fn process_local_variable_target_node_entry(&mut self, node: &LocalVariableTargetNode) {
        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let pattern_capture_type = self
            .pattern_capture_type_stack
            .last()
            .and_then(|captures| captures.get(&variable_name))
            .cloned();
        self.process_local_variable_write(
            node.name().as_slice(),
            node.location(),
            None,
            pattern_capture_type,
        );
    }

    pub fn process_local_variable_target_node_exit(&mut self, _node: &LocalVariableTargetNode) {
        // No-op for now
    }

    // LocalVariableOrWriteNode
    pub fn process_local_variable_or_write_node_entry(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    pub fn process_local_variable_or_write_node_exit(&mut self, _node: &LocalVariableOrWriteNode) {
        // No-op for now
    }

    // LocalVariableAndWriteNode
    pub fn process_local_variable_and_write_node_entry(
        &mut self,
        node: &LocalVariableAndWriteNode,
    ) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    pub fn process_local_variable_and_write_node_exit(
        &mut self,
        _node: &LocalVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // LocalVariableOperatorWriteNode
    pub fn process_local_variable_operator_write_node_entry(
        &mut self,
        node: &LocalVariableOperatorWriteNode,
    ) {
        self.process_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    pub fn process_local_variable_operator_write_node_exit(
        &mut self,
        _node: &LocalVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
