use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, InstanceVariableDependency,
    InstanceVariableObservation, RubyMethod, RubyType,
};
use crate::inference::type_tracker::returns::RecursiveReturnApproximation;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::BTreeSet;

impl TypeTracker {
    /// Infer the type of a return statement
    pub(in crate::inference::type_tracker) fn infer_return(
        &mut self,
        ret: &ReturnNode,
    ) -> RubyType {
        let return_type = if let Some(args) = ret.arguments() {
            let args_list: Vec<_> = args.arguments().iter().collect();
            if let [value] = args_list.as_slice() {
                self.mark_returned_chain_receiver(value);
            }
            if args_list.is_empty() {
                RubyType::nil_class()
            } else if args_list.len() == 1 {
                self.track_node(&args_list[0])
            } else {
                // Multiple return values become an array
                let types: Vec<RubyType> =
                    args_list.iter().map(|a| self.infer_expression(a)).collect();
                RubyType::Array(RubyType::canonical_union_members(types))
            }
        } else {
            RubyType::nil_class()
        };

        let returned_instance_variable = ret.arguments().and_then(|arguments| {
            let mut args = arguments.arguments().iter();
            let first = args.next()?;
            args.next()
                .is_none()
                .then(|| self.returned_instance_variable(&first))
                .flatten()
        });
        if let Some(dependency) = returned_instance_variable {
            self.returns
                .instance_variable_dependencies
                .insert(dependency);
            let approximation = self.instance_variable_return_base(return_type.clone());
            self.returns.explicit_values.push(approximation);
            return return_type;
        }

        let returned_dependency = ret.arguments().and_then(|arguments| {
            let mut args = arguments.arguments().iter();
            let first = args.next();
            let has_exactly_one = first.is_some() && args.next().is_none();
            has_exactly_one
                .then(|| {
                    first.and_then(|value| {
                        let is_direct_recursive_value = value.as_call_node().is_some_and(|call| {
                            call_is_direct_recursive(&call, self.context.method.as_ref())
                        });
                        let dependency = self.return_term_dependency_for_node(&value)?;
                        self.should_track_return_dependency(
                            &dependency.0,
                            &return_type,
                            is_direct_recursive_value,
                        )
                        .then_some(dependency)
                    })
                })
                .flatten()
        });
        let approximation = match returned_dependency {
            Some((dependency, approximation)) => {
                self.returns.dependencies.insert(dependency);
                approximation
            }
            None => {
                let constant_dependencies = ret
                    .arguments()
                    .and_then(|arguments| {
                        let mut args = arguments.arguments().iter();
                        let first = args.next()?;
                        args.next()
                            .is_none()
                            .then(|| self.constant_dependencies_for_node(&first))
                    })
                    .unwrap_or_default();
                if constant_dependencies.is_empty() {
                    RecursiveReturnApproximation::from_ruby_type(return_type.clone())
                } else {
                    self.returns
                        .constant_dependencies
                        .extend(constant_dependencies);
                    self.constant_return_base(return_type.clone())
                }
            }
        };
        self.returns.explicit_values.push(approximation);
        return_type
    }

    pub(in crate::inference::type_tracker) fn constant_return_base(
        &self,
        ruby_type: RubyType,
    ) -> RecursiveReturnApproximation {
        // A retained constant term represents this entire return alternative.
        // During equation collection, its current value is an observation of
        // that dependency, not an independent base. Keeping both would union a
        // stale seed with the final value after resolution or an edit.
        // Direct single-method inference still uses the currently proven type.
        if self.returns.approximation.is_none() || ruby_type == RubyType::Unknown {
            RecursiveReturnApproximation::Bottom
        } else {
            RecursiveReturnApproximation::from_ruby_type(ruby_type)
        }
    }

    /// The instance variable a returned node reads directly, when the
    /// tracked method has a known owner.
    pub(in crate::inference::type_tracker) fn returned_instance_variable(
        &self,
        node: &Node<'_>,
    ) -> Option<InstanceVariableDependency> {
        if let Some(statements) = node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .and_then(|last| self.returned_instance_variable(&last));
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .and_then(|body| self.returned_instance_variable(&body));
        }
        let read = node.as_instance_variable_read_node()?;
        let owner = self.context.instance_variable_owner.clone()?;
        let name = String::from_utf8_lossy(read.name().as_slice());
        Some(InstanceVariableDependency {
            owner,
            name: name.as_ref().into(),
        })
    }

    /// Let the instance variables a returned expression reads take their
    /// file-local type: the one that starts a call chain such as
    /// `@name.strip.upcase`, the operands of `||` and `&&`, and a memoising
    /// `@name ||= value`. A chain through a literal block is left alone: the
    /// block's parameters would carry the provisional type into facts the
    /// engine cannot withhold.
    pub(in crate::inference::type_tracker) fn mark_returned_chain_receiver(
        &mut self,
        node: &Node<'_>,
    ) {
        if let Some(statements) = node.as_statements_node() {
            if let Some(last) = statements.body().iter().last() {
                self.mark_returned_chain_receiver(&last);
            }
            return;
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            if let Some(body) = parentheses.body() {
                self.mark_returned_chain_receiver(&body);
            }
            return;
        }
        if let Some(or_node) = node.as_or_node() {
            self.mark_returned_operand(&or_node.left());
            self.mark_returned_operand(&or_node.right());
            return;
        }
        if let Some(and_node) = node.as_and_node() {
            self.mark_returned_operand(&and_node.left());
            self.mark_returned_operand(&and_node.right());
            return;
        }
        if node.as_instance_variable_or_write_node().is_some() {
            self.returns
                .returned_chain_receivers
                .insert(node.location().start_offset());
            return;
        }
        let mut current = node.as_call_node();
        while let Some(call) = current {
            if call.block().is_some_and(|block| !is_symbol_block(&block)) {
                return;
            }
            let Some(receiver) = call.receiver() else {
                return;
            };
            if receiver.as_instance_variable_read_node().is_some() {
                self.returns
                    .returned_chain_receivers
                    .insert(receiver.location().start_offset());
                return;
            }
            current = receiver.as_call_node();
        }
    }

    fn mark_returned_operand(&mut self, node: &Node<'_>) {
        if node.as_instance_variable_read_node().is_some() {
            self.returns
                .returned_chain_receivers
                .insert(node.location().start_offset());
        } else {
            self.mark_returned_chain_receiver(node);
        }
    }

    /// The file-local type of an instance variable read that starts a
    /// returned call chain, recorded for the engine to confirm.
    pub(in crate::inference::type_tracker) fn returned_chain_receiver_type(
        &self,
        read: &InstanceVariableReadNode<'_>,
    ) -> Option<RubyType> {
        self.marked_instance_variable_type(read.location().start_offset(), read.name().as_slice())
    }

    /// The file-local type of the instance variable a marked node at
    /// `start_offset` names, recorded for the engine to confirm.
    pub(in crate::inference::type_tracker) fn marked_instance_variable_type(
        &self,
        start_offset: usize,
        name: &[u8],
    ) -> Option<RubyType> {
        if !self
            .returns
            .returned_chain_receivers
            .contains(&start_offset)
        {
            return None;
        }
        let owner = self.context.instance_variable_owner.clone()?;
        let name: ustr::Ustr = String::from_utf8_lossy(name).as_ref().into();
        let observed = self.context.instance_variable_types.get(&name)?.clone();
        self.returns
            .instance_variable_observations
            .borrow_mut()
            .insert(InstanceVariableObservation {
                variable: InstanceVariableDependency { owner, name },
                observed: observed.clone(),
                except_memo: None,
            });
        Some(observed)
    }

    /// What every other assignment in this file proves about the variable a
    /// marked `@name ||= value` at `start_offset` memoises, recorded for the
    /// engine to confirm.
    pub(in crate::inference::type_tracker) fn marked_memo_other_assignments_type(
        &self,
        start_offset: usize,
        name: &[u8],
    ) -> Option<RubyType> {
        if !self
            .returns
            .returned_chain_receivers
            .contains(&start_offset)
        {
            return None;
        }
        let owner = self.context.instance_variable_owner.clone()?;
        let (memo, observed) = self
            .context
            .instance_variable_memos
            .get(&start_offset)?
            .clone();
        let name: ustr::Ustr = String::from_utf8_lossy(name).as_ref().into();
        self.returns
            .instance_variable_observations
            .borrow_mut()
            .insert(InstanceVariableObservation {
                variable: InstanceVariableDependency { owner, name },
                observed: observed.clone(),
                except_memo: Some(memo),
            });
        Some(observed)
    }

    /// The return alternative an instance-variable term stands for. While
    /// equations are collected the term alone represents it; direct
    /// single-method inference has only the type the body itself proves.
    pub(in crate::inference::type_tracker) fn instance_variable_return_base(
        &self,
        ruby_type: RubyType,
    ) -> RecursiveReturnApproximation {
        if self.returns.approximation.is_none() {
            RecursiveReturnApproximation::Bottom
        } else {
            RecursiveReturnApproximation::from_ruby_type(ruby_type)
        }
    }

    pub(in crate::inference::type_tracker) fn return_term_dependency_for_call(
        &self,
        call: &CallNode<'_>,
    ) -> Option<(FullyQualifiedName, RecursiveReturnApproximation)> {
        if self
            .returns
            .direct_call_proofs
            .contains(&call.location().start_offset())
        {
            return None;
        }
        let dependency = self.implicit_self_call_fqn(call)?;

        if call_is_direct_recursive(call, self.context.method.as_ref()) {
            if let Some(approximation) = self.returns.approximation.as_ref() {
                return Some((dependency, approximation.clone()));
            }
        }
        Some((dependency, RecursiveReturnApproximation::Bottom))
    }

    pub(in crate::inference::type_tracker) fn should_track_return_dependency(
        &self,
        dependency: &FullyQualifiedName,
        inferred_type: &RubyType,
        direct_recursive: bool,
    ) -> bool {
        direct_recursive
            || *inferred_type == RubyType::Unknown
            || self.analysis.method_candidates.contains(dependency)
            || self
                .analysis
                .project
                .as_ref()
                .is_some_and(|project| project.has_method_return_equation(dependency))
    }

    pub(in crate::inference::type_tracker) fn return_term_dependency_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<(FullyQualifiedName, RecursiveReturnApproximation)> {
        if let Some(statements) = node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .and_then(|last| self.return_term_dependency_for_node(&last));
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .and_then(|body| self.return_term_dependency_for_node(&body));
        }
        if let Some(call) = node.as_call_node() {
            return self.return_term_dependency_for_call(&call);
        }
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.returns.local_terms.get(name.as_ref()).cloned();
        }
        if let Some(write) = node.as_local_variable_write_node() {
            let name = String::from_utf8_lossy(write.name().as_slice());
            return self.returns.local_terms.get(name.as_ref()).cloned();
        }
        None
    }

    pub(in crate::inference::type_tracker) fn constant_dependencies_for_node(
        &self,
        node: &Node<'_>,
    ) -> BTreeSet<ConstantTypeDependency> {
        if let Some(statements) = node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .map(|last| self.constant_dependencies_for_node(&last))
                .unwrap_or_default();
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .map(|body| self.constant_dependencies_for_node(&body))
                .unwrap_or_default();
        }
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.environment.dependencies(name.as_ref());
        }
        if let Some(write) = node.as_local_variable_write_node() {
            let name = String::from_utf8_lossy(write.name().as_slice());
            return self.environment.dependencies(name.as_ref());
        }
        if let Some(call) = node.as_call_node() {
            // A modeled call result (for example generic Array.new) already
            // owns this value. A constructor dependency would erase that proof.
            if self
                .returns
                .direct_call_proofs
                .contains(&call.location().start_offset())
            {
                return BTreeSet::new();
            }
            if call.name().as_slice() == b"new" {
                let Some(receiver) = call.receiver() else {
                    return BTreeSet::new();
                };
                let Some((parts, absolute)) = Self::constant_reference(&receiver) else {
                    return BTreeSet::new();
                };
                let lexical_context = self
                    .context
                    .class
                    .as_ref()
                    .map(FullyQualifiedName::namespace_parts)
                    .unwrap_or_default();
                return BTreeSet::from([ConstantTypeDependency::constructor(
                    parts,
                    absolute,
                    lexical_context,
                )]);
            }
        }
        let Some((parts, absolute)) = Self::constant_reference(node) else {
            return BTreeSet::new();
        };
        let lexical_context = self
            .context
            .class
            .as_ref()
            .map(FullyQualifiedName::namespace_parts)
            .unwrap_or_default();
        BTreeSet::from([ConstantTypeDependency::new(
            parts,
            absolute,
            lexical_context,
        )])
    }
}
/// `&:name` passes no code that could bind the receiver's provisional type.
fn is_symbol_block(block: &Node<'_>) -> bool {
    block
        .as_block_argument_node()
        .and_then(|argument| argument.expression())
        .is_some_and(|expression| expression.as_symbol_node().is_some())
}

pub(in crate::inference::type_tracker) fn call_is_direct_recursive(
    call: &CallNode<'_>,
    method: Option<&RubyMethod>,
) -> bool {
    let Some(method) = method else {
        return false;
    };
    call.name().as_slice() == method.as_str().as_bytes()
        && call
            .receiver()
            .is_none_or(|receiver| receiver.as_self_node().is_some())
}

pub(in crate::inference::type_tracker) fn join_recursive_return_approximations(
    alternatives: impl IntoIterator<Item = RecursiveReturnApproximation>,
) -> RecursiveReturnApproximation {
    let mut proven = Vec::new();
    for alternative in alternatives {
        match alternative {
            RecursiveReturnApproximation::Bottom => {}
            RecursiveReturnApproximation::Proven(ruby_type) => proven.push(ruby_type),
            RecursiveReturnApproximation::Unknown => {
                return RecursiveReturnApproximation::Unknown;
            }
        }
    }
    if proven.is_empty() {
        RecursiveReturnApproximation::Bottom
    } else {
        RecursiveReturnApproximation::Proven(RubyType::union(proven))
    }
}
