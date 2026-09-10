//! Prism dispatch, statement order, and assignment effects.

use crate::core::{RubyType, UnknownReason};
use crate::inference::r#type::literal::literal_shape_construction_unknown_reason;
use crate::inference::type_tracker::flow::branches::ShortCircuitOperator;
use crate::inference::type_tracker::flow::shapes::values::type_is_shape_only;
use crate::inference::type_tracker::returns::dependencies::call_is_direct_recursive;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::BTreeSet;

impl TypeTracker {
    /// Track a program's top-level assignments and control flow.
    pub fn track_program(&mut self, program: &ProgramNode) -> RubyType {
        let stmts = program.statements();
        self.track_statements(&stmts)
    }

    /// Add method parameters to the type environment
    pub(in crate::inference::type_tracker) fn add_parameters(&mut self, _params: &ParametersNode) {
        for (name, ruby_type) in &self.context.parameter_types {
            self.environment.insert(name.clone(), ruby_type.clone());
        }
    }

    /// Track a node and return its type
    ///
    /// This is the main dispatcher that routes to specific tracking methods
    /// based on the node type.
    pub(in crate::inference::type_tracker) fn track_node(&mut self, node: &Node) -> RubyType {
        match node {
            // Statements node - track sequence of statements
            _ if node.as_statements_node().is_some() => {
                let stmts = node.as_statements_node().unwrap();
                self.track_statements(&stmts)
            }

            // Local variable assignment
            _ if node.as_local_variable_write_node().is_some() => {
                let write = node.as_local_variable_write_node().unwrap();
                self.track_assignment(&write)
            }

            // Storing a tracked mutable shape outside the local flow graph is
            // an escape. The assignment expression itself cannot retain a
            // concrete shape after that boundary because arbitrary later code
            // may mutate the stored object.
            _ if node.as_instance_variable_write_node().is_some() => {
                let write = node.as_instance_variable_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_class_variable_write_node().is_some() => {
                let write = node.as_class_variable_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_global_variable_write_node().is_some() => {
                let write = node.as_global_variable_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_constant_write_node().is_some() => {
                let write = node.as_constant_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_constant_path_write_node().is_some() => {
                let write = node.as_constant_path_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }

            // If/unless conditionals
            _ if node.as_if_node().is_some() => {
                let if_node = node.as_if_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_if(&if_node))
            }

            _ if node.as_unless_node().is_some() => {
                let unless_node = node.as_unless_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_unless(&unless_node))
            }

            // Case statement
            _ if node.as_case_node().is_some() => {
                let case_node = node.as_case_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_case(&case_node))
            }

            _ if node.as_case_match_node().is_some() => {
                let case_match_node = node.as_case_match_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_case_match(&case_match_node))
            }

            // Short-circuit boolean expressions are control flow: the right
            // operand may mutate locals, but only one of the skipped/executed
            // environments reaches the following expression.
            _ if node.as_and_node().is_some() => {
                let and_node = node.as_and_node().unwrap();
                self.track_control_flow(|tracker| {
                    tracker.track_short_circuit(
                        &and_node.left(),
                        &and_node.right(),
                        ShortCircuitOperator::And,
                    )
                })
            }

            _ if node.as_or_node().is_some() => {
                let or_node = node.as_or_node().unwrap();
                self.track_control_flow(|tracker| {
                    tracker.track_short_circuit(
                        &or_node.left(),
                        &or_node.right(),
                        ShortCircuitOperator::Or,
                    )
                })
            }

            _ if node.as_super_node().is_some() || node.as_forwarding_super_node().is_some() => {
                self.infer_super_return_type()
            }

            // Begin/rescue/ensure expressions
            _ if node.as_begin_node().is_some() => {
                let begin_node = node.as_begin_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_begin(&begin_node))
            }

            _ if node.as_rescue_modifier_node().is_some() => {
                let rescue_modifier = node.as_rescue_modifier_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_rescue_modifier(&rescue_modifier))
            }

            // Loops
            _ if node.as_while_node().is_some() => {
                let while_node = node.as_while_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_while(&while_node))
            }

            _ if node.as_until_node().is_some() => {
                let until_node = node.as_until_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_until(&until_node))
            }

            // Default: try to infer expression type
            _ => self.infer_expression(node),
        }
    }

    pub(in crate::inference::type_tracker) fn track_control_flow(
        &mut self,
        track: impl FnOnce(&mut Self) -> RubyType,
    ) -> RubyType {
        self.returns.local_terms.clear();
        let was_inside_control_flow = self.returns.inside_control_flow;
        self.returns.inside_control_flow = true;
        self.observations.has_seen_control_flow = true;
        let ruby_type = track(self);
        self.returns.inside_control_flow = was_inside_control_flow;
        self.returns.local_terms.clear();
        ruby_type
    }

    /// Track a sequence of statements and return last expression type
    pub(in crate::inference::type_tracker) fn track_statements(
        &mut self,
        stmts: &StatementsNode,
    ) -> RubyType {
        let mut last_type = RubyType::nil_class();

        for stmt in stmts.body().iter() {
            // Process the statement (this updates self.environment)
            last_type = self.track_node(&stmt);

            // Record state after each statement
            let stmt_end = stmt.location().end_offset();
            self.record_state(stmt_end);
        }

        last_type
    }

    /// Track a local variable assignment
    ///
    /// Infers the type from the value expression and updates the type environment.
    pub(in crate::inference::type_tracker) fn track_assignment(
        &mut self,
        write: &LocalVariableWriteNode,
    ) -> RubyType {
        // Get variable name
        let var_name = String::from_utf8_lossy(write.name().as_slice()).to_string();
        self.invalidate_escaped_callables_in_value(&write.value());

        // The RHS may raise before the local write commits. Every enclosing
        // rescue can therefore observe the prior value (or Ruby's implicit nil
        // for a syntactically declared local that has not yet been assigned).
        if !self.control_flow.rescue_entries.is_empty() {
            let prior_type = self
                .environment
                .types
                .get(&var_name)
                .cloned()
                .unwrap_or_else(RubyType::nil_class);
            self.observe_rescue_entry_type(&var_name, &prior_type);
        }

        // Capture alias provenance before evaluating the RHS. Evaluation may
        // freeze, mutate, or invalidate the referenced identity, but an
        // assignment such as `copy = payload` or `copy = payload.freeze`
        // still binds the exact same object after that effect.
        let value = write.value();
        let aliased_shape_identities = self.shape_identities_for_alias_expression(&value);
        let is_direct_recursive_value = value
            .as_call_node()
            .is_some_and(|call| call_is_direct_recursive(&call, self.context.method.as_ref()));
        let (var_type, assignment_unknown_reason) =
            if let Some(result) = self.infer_collection_literal_type(&value) {
                match result {
                    Ok(ruby_type) => (ruby_type, None),
                    Err(error) => (
                        RubyType::Unknown,
                        Some(literal_shape_construction_unknown_reason(error)),
                    ),
                }
            } else {
                (self.track_node(&value), None)
            };
        let array_shape_aliases = self.array_shape_aliases_for_assignment(&value);
        let constant_dependencies = self.constant_dependencies_for_node(&value);
        let dependency = value
            .as_call_node()
            .and_then(|call| self.return_term_dependency_for_call(&call));
        if let Some(return_type) = self.infer_proc_literal_return_type(&value) {
            self.bind_local_callable(var_name.clone(), return_type);
        } else if let Some(alias) = value.as_local_variable_read_node() {
            let alias_name = String::from_utf8_lossy(alias.name().as_slice()).to_string();
            if let Some(callable) = self.environment.callables.get(&alias_name).cloned() {
                self.bind_local_callable(var_name.clone(), callable);
            } else {
                self.environment.callables.remove(&var_name);
            }
        } else {
            self.environment.callables.remove(&var_name);
        }

        if let Some((dependency, approximation)) = dependency.filter(|(dependency, _)| {
            !self.returns.inside_control_flow
                && self.should_track_return_dependency(
                    dependency,
                    &var_type,
                    is_direct_recursive_value,
                )
        }) {
            self.returns
                .local_terms
                .insert(var_name.clone(), (dependency, approximation));
        } else {
            self.returns.local_terms.remove(&var_name);
        }

        // Update environment. A direct local/freeze alias shares identity;
        // a precise keyed read shares the nested object's contained identity;
        // every other complete shape-producing expression allocates a fresh
        // abstract Hash identity.
        if !aliased_shape_identities.is_empty() {
            self.environment.bind_shape_identities(
                var_name.clone(),
                var_type.clone(),
                aliased_shape_identities,
            );
            self.environment.synchronize_shape_aliases();
        } else if type_is_shape_only(&var_type) {
            let contained_identities =
                self.shape_identities_for_contained_alias_expression(&value, &var_type);
            let identities = if contained_identities.is_empty() {
                let identity = self.allocate_shape_identity(var_type.clone());
                self.link_shape_literal_children(&value, identity);
                if matches!(var_type, RubyType::Shape(_))
                    && self.materialize_direct_shape_children(identity).is_err()
                {
                    self.environment.invalidate_identities(
                        &BTreeSet::from([identity]),
                        UnknownReason::MutableShapeInvalidated,
                    );
                }
                BTreeSet::from([identity])
            } else {
                contained_identities
            };
            self.environment
                .bind_shape_identities(var_name.clone(), var_type.clone(), identities);
            self.environment.synchronize_shape_aliases();
        } else if let Some(reason) = assignment_unknown_reason {
            self.environment.insert_unknown(var_name.clone(), reason);
        } else {
            self.environment.insert(var_name.clone(), var_type.clone());
        }
        if let Some(array_shape_aliases) = array_shape_aliases {
            assert!(
                matches!(var_type, RubyType::Array(_)),
                "INVARIANT VIOLATED: positional Array shape aliases were attached to non-Array type `{var_type}`. This is a bug because only exact Array literals or aliases produce this evidence. Fix: keep array_shape_aliases_for_assignment aligned with collection inference."
            );
            self.environment
                .bind_array_shape_aliases(var_name.clone(), array_shape_aliases);
        }
        self.environment
            .set_constant_dependencies(var_name.clone(), constant_dependencies);
        if !self.control_flow.rescue_entries.is_empty() {
            self.observe_rescue_entry_type(&var_name, &var_type);
        }

        // Return the assigned type (assignments return their value in Ruby).
        // Synchronization may have replaced it with an explained Unknown when
        // the alias limit or an earlier escape invalidated the identity.
        self.environment
            .types
            .get(&var_name)
            .cloned()
            .unwrap_or(var_type)
    }

    pub(in crate::inference::type_tracker) fn track_escaping_write(
        &mut self,
        value: &Node<'_>,
    ) -> RubyType {
        let identities = self.shape_identities_in_escape_expression(value);
        let value_type = self.track_node(value);
        if identities.is_empty() {
            return value_type;
        }
        self.environment
            .invalidate_identities(&identities, UnknownReason::MutableShapeInvalidated);
        RubyType::Unknown
    }
}
