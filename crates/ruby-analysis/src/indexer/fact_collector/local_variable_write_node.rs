use crate::core::{FullyQualifiedName, SymbolKind, TypeFact, TypeProvenance, TypeSubject};
use log::error;
use ruby_prism::{
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, Location, Node,
};

use super::FactCollector;
use crate::inference::RubyType;

impl FactCollector {
    fn parsed_local_variable_name(name: &[u8]) -> Option<String> {
        let variable_name = String::from_utf8_lossy(name).to_string();
        if variable_name.is_empty() {
            error!("Local variable name cannot be empty");
            return None;
        }

        let mut chars = variable_name.chars();
        let first = chars.next().unwrap();

        // Local variables must start with lowercase or underscore
        if !(first.is_lowercase() || first == '_') {
            error!(
                "Local variable name must start with lowercase or _: {}",
                variable_name
            );
            return None;
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
            return None;
        }

        Some(variable_name)
    }

    fn bind_local_callable_from_value(&mut self, variable_name: &str, value: &Node<'_>) {
        if let Some(return_type) = self.infer_known_proc_type(value) {
            self.bind_local_callable(variable_name.to_string(), return_type);
        } else if let Some(alias) = value.as_local_variable_read_node() {
            let alias_name = String::from_utf8_lossy(alias.name().as_slice()).to_string();
            if let Some(callable) = self.proc_return_types_by_local.get(&alias_name).cloned() {
                self.bind_local_callable(variable_name.to_string(), callable);
            } else {
                self.proc_return_types_by_local.remove(variable_name);
            }
        } else {
            self.proc_return_types_by_local.remove(variable_name);
        }
    }

    /// Make the name a local before visiting the RHS so blocks see a variable,
    /// not a method. Lower callables here, before `define_variable` and before
    /// visiting the body, so inner block locals cannot leak into outer_locals.
    /// Do not record the assigned type yet: Ruby evaluates the RHS against the
    /// previous binding, and call proofs are recorded during that visit.
    fn declare_local_variable_write(
        &mut self,
        name: &[u8],
        name_loc: Location,
        value_node: Option<&Node>,
    ) {
        let Some(variable_name) = Self::parsed_local_variable_name(name) else {
            return;
        };
        if let Some(value) = value_node {
            self.invalidate_escaped_callables_in_value(value);
            self.bind_local_callable_from_value(&variable_name, value);
        }

        if let Ok(fqn) = FullyQualifiedName::local_variable(variable_name.clone()) {
            self.direct_push_variable_symbol(fqn, SymbolKind::LocalVariable, &name_loc);
        }

        let location = self.document.prism_location_to_text_range(&name_loc);
        self.document
            .variable_scopes_mut()
            .define_variable(&variable_name, location);
    }

    fn bind_local_variable_write(
        &mut self,
        name: &[u8],
        name_loc: Location,
        value_node: Option<&Node>,
        explicit_type: Option<RubyType>,
    ) {
        let Some(variable_name) = Self::parsed_local_variable_name(name) else {
            return;
        };

        let (inferred_type, inferred_unknown_reason) = if let Some(ty) = explicit_type {
            (ty, None)
        } else if let Some(value) = value_node {
            self.infer_assignment_type_from_value_with_reason(value)
        } else {
            (RubyType::Unknown, None)
        };
        let constant_dependency = value_node.and_then(|value| self.constant_type_dependency(value));

        let location = self.document.prism_location_to_text_range(&name_loc);
        if let Some(reason) = inferred_unknown_reason {
            assert_eq!(
                inferred_type,
                RubyType::Unknown,
                "INVARIANT VIOLATED: local assignment retained shape construction reason `{}` with concrete type `{inferred_type}`. This is a bug because a proof failure and a concrete result cannot describe the same assignment. Fix: return exactly one state from assignment inference.",
                reason.code()
            );
            self.expression_unknown_reasons.insert(location, reason);
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
        self.declare_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_write_node_exit(&mut self, node: &LocalVariableWriteNode) {
        self.bind_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    // LocalVariableTargetNode
    pub fn process_local_variable_target_node_entry(&mut self, node: &LocalVariableTargetNode) {
        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let pattern_capture_type = self
            .pattern_capture_type_stack
            .last()
            .and_then(|captures| captures.get(&variable_name))
            .cloned();
        self.declare_local_variable_write(node.name().as_slice(), node.location(), None);
        self.bind_local_variable_write(
            node.name().as_slice(),
            node.location(),
            None,
            pattern_capture_type,
        );
    }

    pub fn process_local_variable_target_node_exit(&mut self, _node: &LocalVariableTargetNode) {
        // Type facts are bound at entry because this node has no RHS to visit.
    }

    // LocalVariableOrWriteNode
    pub fn process_local_variable_or_write_node_entry(&mut self, node: &LocalVariableOrWriteNode) {
        self.declare_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_or_write_node_exit(&mut self, node: &LocalVariableOrWriteNode) {
        self.bind_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    // LocalVariableAndWriteNode
    pub fn process_local_variable_and_write_node_entry(
        &mut self,
        node: &LocalVariableAndWriteNode,
    ) {
        self.declare_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_and_write_node_exit(&mut self, node: &LocalVariableAndWriteNode) {
        self.bind_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }

    // LocalVariableOperatorWriteNode
    pub fn process_local_variable_operator_write_node_entry(
        &mut self,
        node: &LocalVariableOperatorWriteNode,
    ) {
        self.declare_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_local_variable_operator_write_node_exit(
        &mut self,
        node: &LocalVariableOperatorWriteNode,
    ) {
        self.bind_local_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
            None,
        );
    }
}
