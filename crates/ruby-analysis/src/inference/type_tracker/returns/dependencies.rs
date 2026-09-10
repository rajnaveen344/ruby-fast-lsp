use crate::core::{ConstantTypeDependency, FullyQualifiedName, RubyMethod, RubyType};
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
            if args_list.is_empty() {
                RubyType::nil_class()
            } else if args_list.len() == 1 {
                self.infer_expression(&args_list[0])
            } else {
                // Multiple return values become an array
                let types: Vec<RubyType> =
                    args_list.iter().map(|a| self.infer_expression(a)).collect();
                RubyType::Array(RubyType::canonical_union_members(types))
            }
        } else {
            RubyType::nil_class()
        };

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
                    if return_type == RubyType::Unknown {
                        RecursiveReturnApproximation::Bottom
                    } else {
                        RecursiveReturnApproximation::from_ruby_type(return_type.clone())
                    }
                }
            }
        };
        self.returns.explicit_values.push(approximation);
        return_type
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
                .engine
                .as_ref()
                .is_some_and(|engine| engine.read().has_method_return_equation(dependency))
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
