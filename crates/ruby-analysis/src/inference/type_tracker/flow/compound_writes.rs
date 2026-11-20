//! Compound assignments: `a ||= b`, `a &&= b`, `a += b`, and `@a ||= b`.
//!
//! Each reads the binding it replaces, so its result is proven only when
//! both that binding and the assigned value are.

use crate::core::{RubyType, UnknownReason};
use crate::inference::rbs::CallArguments;
use crate::inference::type_tracker::flow::branches::{
    is_falsy_type, ruby_truthiness, short_circuit_result_type, ShortCircuitOperator, Truthiness,
};
use crate::inference::type_tracker::flow::shapes::values::type_contains_shape;
use crate::inference::type_tracker::observations::CompoundLocalWrite;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

impl TypeTracker {
    /// Track `name ||= value` or `name &&= value`.
    ///
    /// A local with no tracked binding may be an untyped parameter, so it is
    /// never assumed to be nil.
    pub(in crate::inference::type_tracker) fn track_local_short_circuit_write(
        &mut self,
        write_start: usize,
        name: &[u8],
        value: &Node<'_>,
        operator: ShortCircuitOperator,
    ) -> RubyType {
        let name = String::from_utf8_lossy(name).to_string();
        let current = self.current_local_type(&name);
        let value_always_assigned = match (operator, ruby_truthiness(&current)) {
            (ShortCircuitOperator::Or, Truthiness::AlwaysTruthy)
            | (ShortCircuitOperator::And, Truthiness::AlwaysFalsy) => {
                self.observe_compound_write(write_start, &name);
                return current;
            }
            (ShortCircuitOperator::Or, Truthiness::AlwaysFalsy)
            | (ShortCircuitOperator::And, Truthiness::AlwaysTruthy) => true,
            (ShortCircuitOperator::Or | ShortCircuitOperator::And, Truthiness::Conditional) => {
                false
            }
        };

        self.environment
            .callables
            .invalidate_escaped_in_value(value);
        self.observe_rescue_entry_type_when_protected(&name, &current);
        let result = if value_always_assigned {
            let value_type = self.track_escaping_write(value);
            self.environment.callables.assign(&name, None, value);
            value_type
        } else {
            let value_type = self.track_conditional_value(value);
            self.environment.callables.forget(&name);
            if current == RubyType::Unknown || value_type == RubyType::Unknown {
                RubyType::Unknown
            } else {
                short_circuit_result_type(current, value_type, operator)
            }
        };
        let constant_dependencies = if value_always_assigned {
            self.constant_dependencies_for_node(value)
        } else {
            Default::default()
        };
        let bound = self.bind_compound_result(name.clone(), result);
        self.environment
            .set_constant_dependencies(name.clone(), constant_dependencies);
        if value_always_assigned {
            self.observations
                .compound_writes
                .push((write_start, CompoundLocalWrite::AssignsValue));
        } else {
            self.observe_compound_write(write_start, &name);
        }
        bound
    }

    /// Track `name += value` and the other operator assignments as the call
    /// they abbreviate, with the value as its one argument. Collections are
    /// left unknown: their element types would have to follow the argument.
    pub(in crate::inference::type_tracker) fn track_local_operator_write(
        &mut self,
        write: &LocalVariableOperatorWriteNode<'_>,
    ) -> RubyType {
        let name = String::from_utf8_lossy(write.name().as_slice()).to_string();
        let value = write.value();
        let current = self.current_local_type(&name);
        self.environment
            .callables
            .invalidate_escaped_in_value(&value);
        self.observe_rescue_entry_type_when_protected(&name, &current);
        let value_type = self.track_escaping_write(&value);
        self.environment.callables.forget(&name);

        let operator = String::from_utf8_lossy(write.binary_operator().as_slice()).to_string();
        let result = if is_scalar(&current) {
            let arguments = CallArguments {
                positional: vec![value_type],
                block: Some(false),
            };
            self.resolve_static_method_return_outcome(&current, &operator, Some(&arguments))
                .into_proven_type()
                .unwrap_or(RubyType::Unknown)
        } else {
            RubyType::Unknown
        };
        let bound = self.bind_compound_result(name.clone(), result);
        self.observe_compound_write(write.location().start_offset(), &name);
        bound
    }

    /// Track `@name ||= value`. In a returned position the result is the
    /// value assigned here, plus every truthy value another assignment in
    /// this file leaves in the variable; the engine confirms those against
    /// the whole project.
    pub(in crate::inference::type_tracker) fn track_instance_variable_or_write(
        &mut self,
        write: &InstanceVariableOrWriteNode<'_>,
    ) -> RubyType {
        let value_type = self.track_conditional_value(&write.value());
        let Some(observed) = self.marked_memo_other_assignments_type(
            write.location().start_offset(),
            write.name().as_slice(),
        ) else {
            return RubyType::Unknown;
        };
        if observed == RubyType::Unknown || value_type == RubyType::Unknown {
            return RubyType::Unknown;
        }
        let mut alternatives = match observed {
            RubyType::Union(members) => members
                .into_iter()
                .filter(|member| !is_falsy_type(member))
                .collect::<Vec<_>>(),
            single if is_falsy_type(&single) => Vec::new(),
            single => vec![single],
        };
        alternatives.push(value_type);
        RubyType::union(alternatives)
    }

    /// Track a value that only one of two reachable paths evaluates.
    fn track_conditional_value(&mut self, value: &Node<'_>) -> RubyType {
        let skipped = self.environment.clone();
        let value_type = self.track_escaping_write(value);
        let evaluated = std::mem::replace(&mut self.environment, skipped);
        self.merge_env(&evaluated, false);
        value_type
    }

    fn current_local_type(&self, name: &str) -> RubyType {
        self.environment
            .types
            .get(name)
            .cloned()
            .unwrap_or(RubyType::Unknown)
    }

    fn observe_rescue_entry_type_when_protected(&mut self, name: &str, ruby_type: &RubyType) {
        if !self.control_flow.rescue_entries.is_empty() {
            self.observe_rescue_entry_type(name, ruby_type);
        }
    }

    /// Rebind the local. A shape would need an identity this path does not
    /// allocate, so it is bound unknown instead.
    fn bind_compound_result(&mut self, name: String, result: RubyType) -> RubyType {
        self.returns.local_terms.remove(&name);
        let bound = if result == RubyType::Unknown || type_contains_shape(&result) {
            self.environment
                .insert_unknown(name.clone(), UnknownReason::UnresolvedAssignmentValue);
            RubyType::Unknown
        } else {
            self.environment.insert(name.clone(), result.clone());
            result
        };
        self.observe_rescue_entry_type_when_protected(&name, &bound);
        bound
    }

    /// Record what the compound assignment starting at `write_start` left in
    /// `name`, for the collector's binding of that assignment. A binding that
    /// is not fully proven, holds a shape, or rests on a constant resolved
    /// later is recorded unknown.
    fn observe_compound_write(&mut self, write_start: usize, name: &str) {
        let current = self.current_local_type(name);
        let observed = if self.environment.unknown_reason(name).is_some()
            || !self.environment.dependencies(name).is_empty()
            || type_contains_shape(&current)
        {
            RubyType::Unknown
        } else {
            current
        };
        self.observations
            .compound_writes
            .push((write_start, CompoundLocalWrite::Leaves(observed)));
    }
}

/// Whether the value belongs to one class that is not a collection.
fn is_scalar(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Class(_) => true,
        RubyType::Literal(value) => is_scalar(&value.widened_type()),
        RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Union(_)
        | RubyType::Unknown => false,
    }
}
