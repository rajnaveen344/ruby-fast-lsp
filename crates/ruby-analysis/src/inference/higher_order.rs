//! Bounded, signature-driven higher-order call constraints.
//!
//! This module is deliberately independent of Prism and editor protocols. A
//! caller prepares compatible callable signatures from proven receiver and
//! argument types, infers one block body using the returned parameter types,
//! and then supplies the exhaustive block result to finish substitution.

use std::collections::{BTreeMap, BTreeSet};

use rbs_parser::{ParamKind, RbsType};

use crate::core::{
    CallableBlockTemplate, CallableParameterTemplate, CallableSignature,
    CallableTypeTemplate as TypeTemplate, MethodParamKind, RubyType, TypeInferenceOutcome,
    UnknownReason,
};

pub(crate) const MAX_CALLABLE_OVERLOADS: usize = 8;
pub(crate) const MAX_CALLABLE_TYPE_VARIABLES: usize = 8;
pub(crate) const MAX_CALLABLE_BLOCK_PARAMETERS: usize = 4;
pub(crate) const MAX_CALLABLE_SOLVE_ITERATIONS: usize = 16;
pub(crate) const MAX_CALLABLE_TEMPLATE_DEPTH: usize = 8;
pub(crate) const MAX_CALLABLE_UNION_VARIANTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownProcType {
    /// Flow-local identity of the callable literal. Source offsets are unique
    /// within one file traversal and never enter engine storage.
    pub(crate) identity: u32,
    pub(crate) summary: Result<crate::core::CallableBodySummary, UnknownReason>,
}

#[derive(Debug, Clone)]
struct PreparedCallable {
    signature: CallableSignature,
    substitutions: BTreeMap<String, RubyType>,
    solve_iterations: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedCallableSet {
    candidates: Vec<PreparedCallable>,
    block_parameter_types: Vec<RubyType>,
}

impl PreparedCallableSet {
    pub(crate) fn block_parameter_types(&self) -> &[RubyType] {
        &self.block_parameter_types
    }

    pub(crate) fn finish(self, block_return_type: &RubyType) -> TypeInferenceOutcome {
        self.finish_with_block_state(block_return_type, None)
    }

    /// Finish with post-block parameter values only when the caller retained
    /// proof that a block local still aliases the same mutable object. Local
    /// rebinding must pass `None`; it is not mutation of the yielded value.
    pub(crate) fn finish_with_proven_block_state(
        self,
        block_return_type: &RubyType,
        block_parameter_types: &[RubyType],
    ) -> TypeInferenceOutcome {
        self.finish_with_block_state(block_return_type, Some(block_parameter_types))
    }

    fn finish_with_block_state(
        self,
        block_return_type: &RubyType,
        post_block_parameters: Option<&[RubyType]>,
    ) -> TypeInferenceOutcome {
        if RubyType::contains_unknown(block_return_type) {
            return TypeInferenceOutcome::unknown(UnknownReason::IncompleteBlockResult);
        }
        if exceeds_union_bound(block_return_type) {
            return TypeInferenceOutcome::unknown(UnknownReason::HigherOrderBoundExceeded);
        }

        let mut results = Vec::with_capacity(self.candidates.len());
        if post_block_parameters
            .is_some_and(|parameters| parameters.len() != self.block_parameter_types.len())
        {
            return TypeInferenceOutcome::unknown(UnknownReason::IncompleteBlockInput);
        }

        for mut candidate in self.candidates {
            if let Some(post_block_parameters) = post_block_parameters {
                for (template, actual) in candidate
                    .signature
                    .block
                    .parameters
                    .iter()
                    .zip(post_block_parameters)
                {
                    if let Err(reason) = replace_proven_mutated_binding(
                        template,
                        actual,
                        &mut candidate.substitutions,
                    ) {
                        return TypeInferenceOutcome::unknown(reason);
                    }
                }
            }
            if let Err(reason) = constrain_template(
                &candidate.signature.block.return_type,
                block_return_type,
                &mut candidate.substitutions,
                &mut candidate.solve_iterations,
                UnknownReason::IncompleteBlockResult,
            ) {
                return TypeInferenceOutcome::unknown(reason);
            }
            let result = match resolve_template(
                &candidate.signature.return_type,
                &candidate.substitutions,
                1,
            ) {
                Ok(result) if !RubyType::contains_unknown(&result) => result,
                Ok(_) | Err(UnknownReason::IncompleteGenericSubstitution) => {
                    return TypeInferenceOutcome::unknown(
                        UnknownReason::IncompleteGenericSubstitution,
                    )
                }
                Err(reason) => return TypeInferenceOutcome::unknown(reason),
            };
            results.push(result);
        }

        let Some(first) = results.first().cloned() else {
            return TypeInferenceOutcome::unknown(UnknownReason::UnsupportedCallable);
        };
        if results.iter().skip(1).any(|result| result != &first) {
            return TypeInferenceOutcome::unknown(UnknownReason::AmbiguousCallableOverload);
        }
        TypeInferenceOutcome::proven(first)
    }

    /// Lower a static `&:method` target to the same block-result constraint as
    /// an explicit one-parameter block. Symbol#to_proc may forward additional
    /// yielded values as arguments; that call-shape is deliberately rejected
    /// until argument compatibility participates in this proof.
    pub(crate) fn finish_static_method(
        self,
        method_name: &str,
        mut resolve_return: impl FnMut(&RubyType, &str) -> TypeInferenceOutcome,
    ) -> TypeInferenceOutcome {
        if self.block_parameter_types.len() != 1 {
            return TypeInferenceOutcome::unknown(UnknownReason::UnsupportedCallable);
        }
        let receiver_type = self.block_parameter_types[0].clone();
        let return_type = match resolve_return(&receiver_type, method_name).into_proven_type() {
            Some(return_type) => return_type,
            None => return TypeInferenceOutcome::unknown(UnknownReason::IncompleteBlockResult),
        };
        self.finish(&return_type)
    }

    pub(crate) fn finish_known_proc(
        self,
        callable: &KnownProcType,
        capture_type: impl FnMut(&str) -> Option<RubyType>,
        resolve_callable_capture: impl FnMut(&str, &[RubyType]) -> Option<TypeInferenceOutcome>,
        resolve_method: impl FnMut(
            &RubyType,
            &crate::core::RubyMethod,
            &[RubyType],
        ) -> TypeInferenceOutcome,
    ) -> TypeInferenceOutcome {
        let summary = match &callable.summary {
            Ok(summary) => summary,
            Err(reason) => return TypeInferenceOutcome::unknown(*reason),
        };
        let result = crate::inference::callable_body::instantiate_callable_body(
            summary,
            &self.block_parameter_types,
            capture_type,
            resolve_callable_capture,
            resolve_method,
        );
        if let Some(reason) = result.unknown_reason() {
            return TypeInferenceOutcome::unknown(reason);
        }
        let result = result.into_proven_type().expect(
            "INVARIANT VIOLATED: callable-body outcome is neither proven nor Unknown. This is a bug because TypeInferenceOutcome has exactly those two states. Fix: preserve the proof state while composing the higher-order result.",
        );
        self.finish(&result)
    }
}

fn replace_proven_mutated_binding(
    template: &TypeTemplate,
    actual: &RubyType,
    substitutions: &mut BTreeMap<String, RubyType>,
) -> Result<(), UnknownReason> {
    if RubyType::contains_unknown(actual) {
        return Err(UnknownReason::IncompleteBlockResult);
    }
    match template {
        TypeTemplate::Variable(name) => {
            let Some(previous) = substitutions.get(name) else {
                return Err(UnknownReason::IncompleteGenericSubstitution);
            };
            if !matches!(previous, RubyType::Shape(_) | RubyType::Hash(_, _))
                || !matches!(actual, RubyType::Shape(_) | RubyType::Hash(_, _))
            {
                if previous == actual {
                    return Ok(());
                }
                return Err(UnknownReason::UnsupportedCallable);
            }
            substitutions.insert(name.clone(), actual.clone());
            Ok(())
        }
        TypeTemplate::Concrete(_)
        | TypeTemplate::Receiver
        | TypeTemplate::Array(_)
        | TypeTemplate::Hash(_, _)
        | TypeTemplate::Union(_)
        | TypeTemplate::Unconstrained => Ok(()),
    }
}

pub(crate) fn prepare_callable_set(
    receiver_type: Option<&RubyType>,
    signatures: &[CallableSignature],
    receiver_bindings: &[(String, RubyType)],
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, UnknownReason> {
    let applicable = signatures
        .iter()
        .filter(|signature| argument_count_matches(&signature.parameters, argument_types.len()))
        .cloned()
        .collect::<Vec<_>>();
    if applicable.is_empty() {
        return Err(UnknownReason::UnsupportedCallable);
    }
    if applicable.len() > MAX_CALLABLE_OVERLOADS {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }

    let mut candidates = Vec::with_capacity(applicable.len());
    let mut common_block_parameters: Option<Vec<RubyType>> = None;
    for mut signature in applicable {
        instantiate_receiver_signature(&mut signature, receiver_type)?;
        validate_signature(&signature)?;
        let mut substitutions = BTreeMap::new();
        let mut solve_iterations = 0usize;
        for (name, ruby_type) in receiver_bindings {
            if signature
                .type_parameters
                .iter()
                .any(|parameter| parameter == name)
            {
                bind_variable(name, ruby_type, &mut substitutions, &mut solve_iterations)?;
            }
        }
        constrain_arguments(
            &signature.parameters,
            argument_types,
            &mut substitutions,
            &mut solve_iterations,
        )?;
        let block_parameters = signature
            .block
            .parameters
            .iter()
            .map(|parameter| resolve_template(parameter, &substitutions, 1))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|reason| match reason {
                UnknownReason::IncompleteGenericSubstitution => UnknownReason::IncompleteBlockInput,
                other => other,
            })?;
        if block_parameters.iter().any(RubyType::contains_unknown) {
            return Err(UnknownReason::IncompleteBlockInput);
        }
        if let Some(common) = &common_block_parameters {
            if common != &block_parameters {
                return Err(UnknownReason::AmbiguousCallableOverload);
            }
        } else {
            common_block_parameters = Some(block_parameters);
        }
        candidates.push(PreparedCallable {
            signature,
            substitutions,
            solve_iterations,
        });
    }

    Ok(PreparedCallableSet {
        candidates,
        block_parameter_types: common_block_parameters.expect(
            "INVARIANT VIOLATED: applicable callable signatures produced no block parameter set. This is a bug because an empty applicable set returns before preparation. Fix: keep candidate and block-parameter construction atomic.",
        ),
    })
}

fn instantiate_receiver_signature(
    signature: &mut CallableSignature,
    receiver_type: Option<&RubyType>,
) -> Result<(), UnknownReason> {
    for parameter in &mut signature.parameters {
        instantiate_receiver_template(&mut parameter.ruby_type, receiver_type)?;
    }
    for parameter in &mut signature.block.parameters {
        instantiate_receiver_template(parameter, receiver_type)?;
    }
    instantiate_receiver_template(&mut signature.block.return_type, receiver_type)?;
    instantiate_receiver_template(&mut signature.return_type, receiver_type)
}

fn instantiate_receiver_template(
    template: &mut TypeTemplate,
    receiver_type: Option<&RubyType>,
) -> Result<(), UnknownReason> {
    match template {
        TypeTemplate::Receiver => {
            let receiver = receiver_type
                .filter(|receiver| !RubyType::contains_unknown(receiver))
                .ok_or(UnknownReason::IncompleteBlockInput)?;
            *template = TypeTemplate::Concrete(receiver.clone());
            Ok(())
        }
        TypeTemplate::Array(element) => instantiate_receiver_template(element, receiver_type),
        TypeTemplate::Hash(key, value) => {
            instantiate_receiver_template(key, receiver_type)?;
            instantiate_receiver_template(value, receiver_type)
        }
        TypeTemplate::Union(members) => {
            for member in members {
                instantiate_receiver_template(member, receiver_type)?;
            }
            Ok(())
        }
        TypeTemplate::Concrete(_) | TypeTemplate::Variable(_) | TypeTemplate::Unconstrained => {
            Ok(())
        }
    }
}

pub(crate) fn callable_signature_from_rbs(
    receiver_type_parameters: &[String],
    method_type_parameters: &[String],
    method: &rbs_parser::MethodType,
) -> Result<Option<CallableSignature>, UnknownReason> {
    let Some(block) = method.block.as_ref() else {
        return Ok(None);
    };
    let mut type_parameters = receiver_type_parameters.to_vec();
    type_parameters.extend_from_slice(method_type_parameters);
    let type_parameter_set = type_parameters.iter().cloned().collect::<BTreeSet<_>>();
    let parameters = method
        .params
        .iter()
        .map(|parameter| {
            Ok(CallableParameterTemplate {
                kind: method_param_kind(&parameter.kind),
                ruby_type: type_template_from_rbs(&parameter.r#type, &type_parameter_set, 1)?,
            })
        })
        .collect::<Result<Vec<_>, UnknownReason>>()?;
    let block_parameters = block
        .params
        .iter()
        .map(|parameter| type_template_from_rbs(&parameter.r#type, &type_parameter_set, 1))
        .collect::<Result<Vec<_>, _>>()?;
    let signature = CallableSignature {
        receiver_type_parameters: receiver_type_parameters.to_vec(),
        type_parameters,
        parameters,
        block: CallableBlockTemplate {
            parameters: block_parameters,
            return_type: type_template_from_rbs(&block.return_type, &type_parameter_set, 1)?,
            required: block.required,
        },
        return_type: type_template_from_rbs(&method.return_type, &type_parameter_set, 1)?,
    };
    validate_signature(&signature)?;
    Ok(Some(signature))
}

fn validate_signature(signature: &CallableSignature) -> Result<(), UnknownReason> {
    if signature.type_parameters.len() > MAX_CALLABLE_TYPE_VARIABLES
        || signature.block.parameters.len() > MAX_CALLABLE_BLOCK_PARAMETERS
    {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }
    let unique = signature.type_parameters.iter().collect::<BTreeSet<_>>();
    if unique.len() != signature.type_parameters.len() {
        return Err(UnknownReason::UnsupportedCallable);
    }
    if signature.receiver_type_parameters.len() > signature.type_parameters.len()
        || signature
            .receiver_type_parameters
            .iter()
            .zip(&signature.type_parameters)
            .any(|(receiver, parameter)| receiver != parameter)
    {
        return Err(UnknownReason::UnsupportedCallable);
    }
    validate_template_depth(&signature.return_type, 1)?;
    validate_template_depth(&signature.block.return_type, 1)?;
    for parameter in &signature.parameters {
        validate_template_depth(&parameter.ruby_type, 1)?;
    }
    for parameter in &signature.block.parameters {
        validate_template_depth(parameter, 1)?;
    }
    Ok(())
}

fn validate_template_depth(template: &TypeTemplate, depth: usize) -> Result<(), UnknownReason> {
    if depth > MAX_CALLABLE_TEMPLATE_DEPTH {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }
    match template {
        TypeTemplate::Array(element) => validate_template_depth(element, depth + 1),
        TypeTemplate::Hash(key, value) => {
            validate_template_depth(key, depth + 1)?;
            validate_template_depth(value, depth + 1)
        }
        TypeTemplate::Union(members) => {
            if members.len() > MAX_CALLABLE_UNION_VARIANTS {
                return Err(UnknownReason::HigherOrderBoundExceeded);
            }
            for member in members {
                validate_template_depth(member, depth + 1)?;
            }
            Ok(())
        }
        TypeTemplate::Concrete(_)
        | TypeTemplate::Receiver
        | TypeTemplate::Variable(_)
        | TypeTemplate::Unconstrained => Ok(()),
    }
}

fn type_template_from_rbs(
    rbs_type: &RbsType,
    type_parameters: &BTreeSet<String>,
    depth: usize,
) -> Result<TypeTemplate, UnknownReason> {
    if depth > MAX_CALLABLE_TEMPLATE_DEPTH {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }
    match rbs_type {
        RbsType::TypeVar(name) => Ok(TypeTemplate::Variable(name.clone())),
        RbsType::Class(name) if type_parameters.contains(name) => {
            Ok(TypeTemplate::Variable(name.clone()))
        }
        RbsType::Class(name) if name == "boolish" => Ok(TypeTemplate::Unconstrained),
        RbsType::ClassInstance { name, args }
            if args.is_empty()
                && type_parameters.contains(name.strip_prefix("::").unwrap_or(name)) =>
        {
            Ok(TypeTemplate::Variable(
                name.strip_prefix("::").unwrap_or(name).to_string(),
            ))
        }
        RbsType::ClassInstance { name, args } => {
            let name = name.strip_prefix("::").unwrap_or(name);
            match (name, args.as_slice()) {
                ("Array", [element]) => Ok(TypeTemplate::Array(Box::new(type_template_from_rbs(
                    element,
                    type_parameters,
                    depth + 1,
                )?))),
                ("Hash", [key, value]) => Ok(TypeTemplate::Hash(
                    Box::new(type_template_from_rbs(key, type_parameters, depth + 1)?),
                    Box::new(type_template_from_rbs(value, type_parameters, depth + 1)?),
                )),
                ("Array" | "Hash", _) => Err(UnknownReason::UnsupportedCallable),
                _ => concrete_template(rbs_type),
            }
        }
        RbsType::Union(members) => {
            if members.len() > MAX_CALLABLE_UNION_VARIANTS {
                return Err(UnknownReason::HigherOrderBoundExceeded);
            }
            Ok(TypeTemplate::Union(
                members
                    .iter()
                    .map(|member| type_template_from_rbs(member, type_parameters, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        RbsType::Optional(inner) => Ok(TypeTemplate::Union(vec![
            TypeTemplate::Concrete(RubyType::nil_class()),
            type_template_from_rbs(inner, type_parameters, depth + 1)?,
        ])),
        RbsType::Untyped | RbsType::Top | RbsType::Bot => Ok(TypeTemplate::Unconstrained),
        RbsType::SelfType => Ok(TypeTemplate::Receiver),
        RbsType::Proc { .. }
        | RbsType::Instance
        | RbsType::ClassType
        | RbsType::Intersection(_)
        | RbsType::Tuple(_)
        | RbsType::Interface(_) => Err(UnknownReason::UnsupportedCallable),
        RbsType::Void
        | RbsType::Nil
        | RbsType::Bool
        | RbsType::Class(_)
        | RbsType::Record(_)
        | RbsType::Literal(_) => concrete_template(rbs_type),
    }
}

fn concrete_template(rbs_type: &RbsType) -> Result<TypeTemplate, UnknownReason> {
    let ruby_type = crate::inference::rbs::rbs_type_to_ruby_type(rbs_type);
    if RubyType::contains_unknown(&ruby_type) {
        Err(UnknownReason::UnsupportedCallable)
    } else {
        Ok(TypeTemplate::Concrete(ruby_type))
    }
}

fn method_param_kind(kind: &ParamKind) -> MethodParamKind {
    match kind {
        ParamKind::Required => MethodParamKind::Required,
        ParamKind::Optional => MethodParamKind::Optional,
        ParamKind::Rest => MethodParamKind::Rest,
        ParamKind::Keyword => MethodParamKind::RequiredKeyword,
        ParamKind::KeywordOpt => MethodParamKind::OptionalKeyword,
        ParamKind::KeywordRest => MethodParamKind::KeywordRest,
        ParamKind::Block => MethodParamKind::Block,
    }
}

fn argument_count_matches(parameters: &[CallableParameterTemplate], count: usize) -> bool {
    let required = parameters
        .iter()
        .filter(|parameter| parameter.kind == MethodParamKind::Required)
        .count();
    let optional = parameters
        .iter()
        .filter(|parameter| parameter.kind == MethodParamKind::Optional)
        .count();
    let has_rest = parameters
        .iter()
        .any(|parameter| parameter.kind == MethodParamKind::Rest);
    count >= required && (has_rest || count <= required + optional)
}

fn constrain_arguments(
    parameters: &[CallableParameterTemplate],
    arguments: &[RubyType],
    substitutions: &mut BTreeMap<String, RubyType>,
    solve_iterations: &mut usize,
) -> Result<(), UnknownReason> {
    let positional = parameters
        .iter()
        .filter(|parameter| {
            matches!(
                parameter.kind,
                MethodParamKind::Required | MethodParamKind::Optional | MethodParamKind::Rest
            )
        })
        .collect::<Vec<_>>();
    let rest = positional
        .iter()
        .position(|parameter| parameter.kind == MethodParamKind::Rest);
    for (index, argument) in arguments.iter().enumerate() {
        let parameter = positional
            .get(index)
            .copied()
            .or_else(|| rest.and_then(|rest_index| positional.get(rest_index).copied()))
            .ok_or(UnknownReason::UnsupportedCallable)?;
        constrain_template(
            &parameter.ruby_type,
            argument,
            substitutions,
            solve_iterations,
            UnknownReason::IncompleteGenericSubstitution,
        )?;
    }
    Ok(())
}

fn constrain_template(
    template: &TypeTemplate,
    actual: &RubyType,
    substitutions: &mut BTreeMap<String, RubyType>,
    solve_iterations: &mut usize,
    incomplete_reason: UnknownReason,
) -> Result<(), UnknownReason> {
    if RubyType::contains_unknown(actual) {
        return Err(incomplete_reason);
    }
    match template {
        TypeTemplate::Variable(name) => {
            bind_variable(name, actual, substitutions, solve_iterations)
        }
        TypeTemplate::Receiver => panic!(
            "INVARIANT VIOLATED: an uninstantiated receiver template reached callable constraints. This is a bug because prepare_callable_set must replace every receiver template before solving. Fix: recurse through every callable template in instantiate_receiver_signature."
        ),
        TypeTemplate::Concrete(expected) => {
            if expected.is_compatible_with(actual) {
                Ok(())
            } else {
                Err(UnknownReason::UnsupportedCallable)
            }
        }
        TypeTemplate::Array(element) => {
            let RubyType::Array(elements) = actual else {
                return Err(UnknownReason::UnsupportedCallable);
            };
            let element_type = RubyType::union(elements.clone());
            constrain_template(
                element,
                &element_type,
                substitutions,
                solve_iterations,
                incomplete_reason,
            )
        }
        TypeTemplate::Hash(key, value) => {
            let generic = match actual {
                RubyType::Shape(shape) => shape.generic_hash_type(),
                RubyType::Hash(_, _) => actual.clone(),
                RubyType::Class(_)
                | RubyType::Module(_)
                | RubyType::ClassReference(_)
                | RubyType::ModuleReference(_)
                | RubyType::Literal(_)
                | RubyType::Array(_)
                | RubyType::Union(_)
                | RubyType::Unknown => return Err(UnknownReason::UnsupportedCallable),
            };
            let RubyType::Hash(keys, values) = generic else {
                panic!(
                    "INVARIANT VIOLATED: a generic Hash projection returned a non-Hash type. This is a bug because structural shapes project only to RubyType::Hash. Fix: keep ShapeType::generic_hash_type canonical."
                );
            };
            constrain_template(
                key,
                &RubyType::union(keys),
                substitutions,
                solve_iterations,
                incomplete_reason,
            )?;
            constrain_template(
                value,
                &RubyType::union(values),
                substitutions,
                solve_iterations,
                incomplete_reason,
            )
        }
        TypeTemplate::Union(members) => {
            constrain_union(members, actual, substitutions, solve_iterations)
        }
        TypeTemplate::Unconstrained => Ok(()),
    }
}

fn constrain_union(
    templates: &[TypeTemplate],
    actual: &RubyType,
    substitutions: &mut BTreeMap<String, RubyType>,
    solve_iterations: &mut usize,
) -> Result<(), UnknownReason> {
    let variables = templates
        .iter()
        .filter_map(|template| match template {
            TypeTemplate::Variable(name) => Some(name.as_str()),
            TypeTemplate::Concrete(_)
            | TypeTemplate::Receiver
            | TypeTemplate::Array(_)
            | TypeTemplate::Hash(_, _)
            | TypeTemplate::Union(_)
            | TypeTemplate::Unconstrained => None,
        })
        .collect::<Vec<_>>();
    if variables.len() > 1 {
        return Err(UnknownReason::UnsupportedCallable);
    }
    let actual_members = match actual {
        RubyType::Union(members) => members.clone(),
        other => vec![other.clone()],
    };
    let mut unmatched = Vec::new();
    for actual_member in actual_members {
        let matches_non_variable = templates.iter().any(|template| match template {
            TypeTemplate::Concrete(expected) => expected.is_compatible_with(&actual_member),
            TypeTemplate::Unconstrained => true,
            TypeTemplate::Variable(_)
            | TypeTemplate::Receiver
            | TypeTemplate::Array(_)
            | TypeTemplate::Hash(_, _)
            | TypeTemplate::Union(_) => false,
        });
        if !matches_non_variable {
            unmatched.push(actual_member);
        }
    }
    if let Some(variable) = variables.first() {
        if unmatched.is_empty() {
            return Ok(());
        }
        return bind_variable(
            variable,
            &RubyType::union(unmatched),
            substitutions,
            solve_iterations,
        );
    }
    if unmatched.is_empty() {
        Ok(())
    } else {
        Err(UnknownReason::UnsupportedCallable)
    }
}

fn bind_variable(
    name: &str,
    actual: &RubyType,
    substitutions: &mut BTreeMap<String, RubyType>,
    solve_iterations: &mut usize,
) -> Result<(), UnknownReason> {
    if RubyType::contains_unknown(actual) {
        return Err(UnknownReason::IncompleteGenericSubstitution);
    }
    *solve_iterations = solve_iterations.checked_add(1).expect(
        "INVARIANT VIOLATED: higher-order solve iteration count overflowed usize. This is a bug because the solver stops at a tiny fixed bound. Fix: increment only through bind_variable and preserve the bound check.",
    );
    if *solve_iterations > MAX_CALLABLE_SOLVE_ITERATIONS {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }
    let next = substitutions.get(name).map_or_else(
        || actual.clone(),
        |existing| RubyType::union([existing.clone(), actual.clone()]),
    );
    if exceeds_union_bound(&next) {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }
    substitutions.insert(name.to_string(), next);
    Ok(())
}

fn resolve_template(
    template: &TypeTemplate,
    substitutions: &BTreeMap<String, RubyType>,
    depth: usize,
) -> Result<RubyType, UnknownReason> {
    if depth > MAX_CALLABLE_TEMPLATE_DEPTH {
        return Err(UnknownReason::HigherOrderBoundExceeded);
    }
    match template {
        TypeTemplate::Concrete(ruby_type) => Ok(ruby_type.clone()),
        TypeTemplate::Receiver => panic!(
            "INVARIANT VIOLATED: an uninstantiated receiver template reached callable result substitution. This is a bug because prepare_callable_set must replace every receiver template before solving. Fix: recurse through every callable template in instantiate_receiver_signature."
        ),
        TypeTemplate::Variable(name) => substitutions
            .get(name)
            .cloned()
            .ok_or(UnknownReason::IncompleteGenericSubstitution),
        TypeTemplate::Array(element) => Ok(RubyType::array_of(resolve_template(
            element,
            substitutions,
            depth + 1,
        )?)),
        TypeTemplate::Hash(key, value) => Ok(RubyType::hash_of(
            resolve_template(key, substitutions, depth + 1)?,
            resolve_template(value, substitutions, depth + 1)?,
        )),
        TypeTemplate::Union(members) => {
            if members.len() > MAX_CALLABLE_UNION_VARIANTS {
                return Err(UnknownReason::HigherOrderBoundExceeded);
            }
            let resolved = members
                .iter()
                .map(|member| resolve_template(member, substitutions, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RubyType::union(resolved))
        }
        TypeTemplate::Unconstrained => Err(UnknownReason::IncompleteGenericSubstitution),
    }
}

fn exceeds_union_bound(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Union(members) => {
            members.len() > MAX_CALLABLE_UNION_VARIANTS || members.iter().any(exceeds_union_bound)
        }
        RubyType::Array(elements) => elements.iter().any(exceeds_union_bound),
        RubyType::Hash(keys, values) => {
            keys.iter().any(exceeds_union_bound) || values.iter().any(exceeds_union_bound)
        }
        RubyType::Shape(shape) => shape
            .fields()
            .iter()
            .any(|field| exceeds_union_bound(field.value())),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
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
}
