use super::*;
use crate::core::FullyQualifiedName;

fn variable(name: &str) -> TypeTemplate {
    TypeTemplate::Variable(name.to_string())
}

fn map_signature() -> CallableSignature {
    CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec!["Elem".to_string(), "Output".to_string()],
        parameters: Vec::new(),
        block: CallableBlockTemplate {
            parameters: vec![variable("Elem")],
            return_type: variable("Output"),
            required: true,
        },
        return_type: TypeTemplate::Array(Box::new(variable("Output"))),
    }
}

#[test]
fn map_substitutes_receiver_and_block_results() {
    let prepared = prepare_callable_set(
        None,
        &[map_signature()],
        &[("Elem".to_string(), RubyType::integer())],
        &[],
    )
    .expect("the complete synthetic map signature must prepare");
    assert_eq!(prepared.block_parameter_types(), &[RubyType::integer()]);
    assert_eq!(
        prepared.finish(&RubyType::string()).proven_type(),
        Some(&RubyType::array_of(RubyType::string()))
    );
}

#[test]
fn filter_map_subtracts_declared_falsey_members_before_binding() {
    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec!["Elem".to_string(), "Output".to_string()],
        parameters: Vec::new(),
        block: CallableBlockTemplate {
            parameters: vec![variable("Elem")],
            return_type: TypeTemplate::Union(vec![
                TypeTemplate::Concrete(RubyType::nil_class()),
                TypeTemplate::Concrete(RubyType::false_class()),
                variable("Output"),
            ]),
            required: true,
        },
        return_type: TypeTemplate::Array(Box::new(variable("Output"))),
    };
    let prepared = prepare_callable_set(
        None,
        &[signature],
        &[("Elem".to_string(), RubyType::integer())],
        &[],
    )
    .expect("the complete filter_map signature must prepare");
    let block_result = RubyType::union([RubyType::nil_class(), RubyType::string()]);
    assert_eq!(
        prepared.finish(&block_result).proven_type(),
        Some(&RubyType::array_of(RubyType::string()))
    );
}

#[test]
fn one_unknown_argument_fails_before_a_partial_result_can_escape() {
    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec!["Accumulator".to_string()],
        parameters: vec![CallableParameterTemplate {
            kind: MethodParamKind::Required,
            ruby_type: variable("Accumulator"),
        }],
        block: CallableBlockTemplate {
            parameters: vec![variable("Accumulator")],
            return_type: TypeTemplate::Unconstrained,
            required: true,
        },
        return_type: variable("Accumulator"),
    };
    assert_eq!(
        prepare_callable_set(None, &[signature], &[], &[RubyType::Unknown]).unwrap_err(),
        UnknownReason::IncompleteGenericSubstitution
    );
}

#[test]
fn conflicting_overloads_are_ambiguous() {
    let mut string_result = map_signature();
    string_result.return_type = TypeTemplate::Concrete(RubyType::string());
    let mut integer_result = map_signature();
    integer_result.return_type = TypeTemplate::Concrete(RubyType::integer());
    let prepared = prepare_callable_set(
        None,
        &[string_result, integer_result],
        &[("Elem".to_string(), RubyType::integer())],
        &[],
    )
    .expect("compatible overloads with equal block inputs must prepare together");
    assert_eq!(
        prepared.finish(&RubyType::string()).unknown_reason(),
        Some(UnknownReason::AmbiguousCallableOverload)
    );
}

#[test]
fn overload_bound_fails_closed() {
    let signatures = (0..=MAX_CALLABLE_OVERLOADS)
        .map(|_| map_signature())
        .collect::<Vec<_>>();
    assert_eq!(
        prepare_callable_set(
            None,
            &signatures,
            &[("Elem".to_string(), RubyType::integer())],
            &[],
        )
        .unwrap_err(),
        UnknownReason::HigherOrderBoundExceeded
    );
}

#[test]
fn identical_compatible_overloads_produce_one_canonical_result() {
    let prepared = prepare_callable_set(
        None,
        &[map_signature(), map_signature()],
        &[("Elem".to_string(), RubyType::integer())],
        &[],
    )
    .expect("identical compatible overloads must prepare together");
    assert_eq!(
        prepared.finish(&RubyType::string()).proven_type(),
        Some(&RubyType::array_of(RubyType::string()))
    );
}

#[test]
fn missing_return_binding_fails_closed() {
    let mut signature = map_signature();
    signature.type_parameters.push("Unbound".to_string());
    signature.return_type = variable("Unbound");
    let prepared = prepare_callable_set(
        None,
        &[signature],
        &[("Elem".to_string(), RubyType::integer())],
        &[],
    )
    .expect("the missing binding is a finish-time proof failure");
    assert_eq!(
        prepared.finish(&RubyType::string()).unknown_reason(),
        Some(UnknownReason::IncompleteGenericSubstitution)
    );
}

#[test]
fn recursive_array_templates_substitute_without_flattening() {
    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec!["Value".to_string()],
        parameters: vec![CallableParameterTemplate {
            kind: MethodParamKind::Required,
            ruby_type: TypeTemplate::Array(Box::new(TypeTemplate::Array(Box::new(variable(
                "Value",
            ))))),
        }],
        block: CallableBlockTemplate {
            parameters: vec![variable("Value")],
            return_type: TypeTemplate::Unconstrained,
            required: true,
        },
        return_type: TypeTemplate::Array(Box::new(variable("Value"))),
    };
    let argument = RubyType::array_of(RubyType::array_of(RubyType::string()));
    let prepared = prepare_callable_set(None, &[signature], &[], &[argument])
        .expect("the bounded nested template must prepare");
    assert_eq!(prepared.block_parameter_types(), &[RubyType::string()]);
}

#[test]
fn type_variable_bound_fails_closed() {
    let mut signature = map_signature();
    signature.type_parameters = (0..=MAX_CALLABLE_TYPE_VARIABLES)
        .map(|index| format!("T{index}"))
        .collect();
    assert_eq!(
        prepare_callable_set(None, &[signature], &[], &[]).unwrap_err(),
        UnknownReason::HigherOrderBoundExceeded
    );
}

#[test]
fn block_parameter_bound_fails_closed() {
    let mut signature = map_signature();
    signature.block.parameters = (0..=MAX_CALLABLE_BLOCK_PARAMETERS)
        .map(|_| TypeTemplate::Concrete(RubyType::integer()))
        .collect();
    assert_eq!(
        prepare_callable_set(
            None,
            &[signature],
            &[("Elem".to_string(), RubyType::integer())],
            &[],
        )
        .unwrap_err(),
        UnknownReason::HigherOrderBoundExceeded
    );
}

#[test]
fn solve_iteration_bound_fails_closed() {
    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec!["Value".to_string()],
        parameters: (0..=MAX_CALLABLE_SOLVE_ITERATIONS)
            .map(|_| CallableParameterTemplate {
                kind: MethodParamKind::Required,
                ruby_type: variable("Value"),
            })
            .collect(),
        block: CallableBlockTemplate {
            parameters: vec![variable("Value")],
            return_type: TypeTemplate::Unconstrained,
            required: true,
        },
        return_type: variable("Value"),
    };
    let arguments = (0..=MAX_CALLABLE_SOLVE_ITERATIONS)
        .map(|_| RubyType::string())
        .collect::<Vec<_>>();
    assert_eq!(
        prepare_callable_set(None, &[signature], &[], &arguments).unwrap_err(),
        UnknownReason::HigherOrderBoundExceeded
    );
}

#[test]
fn callable_template_depth_bound_fails_closed() {
    let mut nested = variable("Output");
    for _ in 0..MAX_CALLABLE_TEMPLATE_DEPTH {
        nested = TypeTemplate::Array(Box::new(nested));
    }
    let mut signature = map_signature();
    signature.return_type = nested;
    assert_eq!(
        prepare_callable_set(
            None,
            &[signature],
            &[("Elem".to_string(), RubyType::integer())],
            &[],
        )
        .unwrap_err(),
        UnknownReason::HigherOrderBoundExceeded
    );
}

#[test]
fn callable_union_variant_bound_fails_closed() {
    let members = (0..=MAX_CALLABLE_UNION_VARIANTS)
        .map(|index| {
            let name = format!("Variant{index}");
            TypeTemplate::Concrete(RubyType::Class(
                FullyQualifiedName::try_from(name.as_str())
                    .expect("the synthetic variant name must be a valid constant"),
            ))
        })
        .collect();
    let mut signature = map_signature();
    signature.return_type = TypeTemplate::Union(members);
    assert_eq!(
        prepare_callable_set(
            None,
            &[signature],
            &[("Elem".to_string(), RubyType::integer())],
            &[],
        )
        .unwrap_err(),
        UnknownReason::HigherOrderBoundExceeded
    );
}

#[test]
fn strict_known_proc_arity_mismatch_fails_closed() {
    let prepared = prepare_callable_set(
        None,
        &[map_signature()],
        &[("Elem".to_string(), RubyType::integer())],
        &[],
    )
    .expect("the map signature must prepare");
    let callable = KnownProcType {
        identity: 1,
        summary: Ok(crate::core::CallableBodySummary {
            strict_arity: true,
            parameters: vec![
                crate::core::CallableBodyParameter {
                    name: "left".to_string(),
                    kind: crate::core::CallableBodyParameterKind::Required,
                    default: None,
                },
                crate::core::CallableBodyParameter {
                    name: "right".to_string(),
                    kind: crate::core::CallableBodyParameterKind::Required,
                    default: None,
                },
            ],
            captures: Vec::new(),
            result: crate::core::CallableBodyExpression::Literal(RubyType::string()),
            node_count: 1,
        }),
    };
    assert_eq!(
        prepared
            .finish_known_proc(
                &callable,
                |_| None,
                |_, _| None,
                |_, _, _| TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn),
            )
            .unknown_reason(),
        Some(UnknownReason::IncompleteCallableInput)
    );
}
