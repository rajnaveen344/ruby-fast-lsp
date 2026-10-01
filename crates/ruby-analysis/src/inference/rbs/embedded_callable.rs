//! Embedded RBS callable owner lookup: receiver identity, generic bindings, and ancestor edges.

use rbs_parser::{Loader, RbsType};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::conversion::{rbs_type_to_class_name, substitute_rbs_edge_argument};
use super::RBS_LOADER;
use crate::core::callables::callable_signature::CallableSignature;
use crate::core::RubyType;
use crate::inference::higher_order::{
    callable_signature_from_rbs, prepare_callable_set, PreparedCallableSet,
};

#[derive(Debug)]
struct RbsCallableOwner {
    signatures: Vec<CallableSignature>,
    receiver_bindings: Vec<(String, RubyType)>,
}

/// Prepare a block-bearing embedded RBS method through the same bounded
/// higher-order solver used by project signatures.
pub(crate) fn prepare_rbs_higher_order_call(
    receiver_type: &RubyType,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let (class_name, type_arguments) = rbs_receiver_identity(receiver_type)
        .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
    let loader = RBS_LOADER.read();
    let root_bindings = declaration_bindings(
        declaration_type_parameters(&loader, &class_name)
            .ok_or(crate::core::UnknownReason::UnsupportedCallable)?,
        &type_arguments,
    )?;
    let mut visited = BTreeSet::new();
    let owner = find_rbs_callable_owner(
        &loader,
        &class_name,
        method_name,
        root_bindings,
        &mut visited,
    )?
    .ok_or(crate::core::UnknownReason::UnsupportedCallable)?;
    prepare_callable_set(
        Some(receiver_type),
        &owner.signatures,
        &owner.receiver_bindings,
        argument_types,
    )
}

pub(super) fn rbs_receiver_identity(receiver_type: &RubyType) -> Option<(String, Vec<RubyType>)> {
    match receiver_type {
        RubyType::Array(elements) => {
            Some(("Array".to_string(), vec![RubyType::union(elements.clone())]))
        }
        RubyType::Hash(keys, values) => Some((
            "Hash".to_string(),
            vec![
                RubyType::union(keys.clone()),
                RubyType::union(values.clone()),
            ],
        )),
        RubyType::Shape(shape) => rbs_receiver_identity(&shape.generic_hash_type()),
        RubyType::Literal(value) => rbs_receiver_identity(&value.widened_type()),
        RubyType::Class(fqn)
        | RubyType::Module(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::ModuleReference(fqn) => fqn
            .namespace_parts()
            .last()
            .map(|name| (name.to_string(), Vec::new())),
        RubyType::Union(_) | RubyType::Unknown => None,
    }
}

fn declaration_type_parameters<'a>(
    loader: &'a Loader,
    owner: &str,
) -> Option<&'a [rbs_parser::TypeParam]> {
    if let Some(class) = loader.get_class(owner) {
        return Some(&class.type_params);
    }
    loader
        .get_module(owner)
        .map(|module| module.type_params.as_slice())
}

fn normalized_type_parameter_name(parameter: &rbs_parser::TypeParam) -> String {
    parameter
        .name
        .split_whitespace()
        .last()
        .unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: an RBS type parameter has no non-whitespace name. This is a bug because the parser accepted an unusable generic binding. Fix: reject empty type-parameter names during RBS conversion."
            )
        })
        .to_string()
}

fn declaration_bindings(
    parameters: &[rbs_parser::TypeParam],
    arguments: &[RubyType],
) -> Result<BTreeMap<String, RubyType>, crate::core::UnknownReason> {
    if parameters.len() != arguments.len() {
        if parameters.is_empty() && arguments.is_empty() {
            return Ok(BTreeMap::new());
        }
        return Err(crate::core::UnknownReason::IncompleteBlockInput);
    }
    let mut bindings = BTreeMap::new();
    for (parameter, argument) in parameters.iter().zip(arguments) {
        if RubyType::contains_unknown(argument) {
            return Err(crate::core::UnknownReason::IncompleteBlockInput);
        }
        let name = normalized_type_parameter_name(parameter);
        assert!(
            bindings.insert(name.clone(), argument.clone()).is_none(),
            "INVARIANT VIOLATED: RBS declaration repeats type parameter `{name}`. This is a bug because one generic variable cannot have two declaration bindings. Fix: reject duplicate RBS type parameters before callable solving."
        );
    }
    Ok(bindings)
}

fn find_rbs_callable_owner(
    loader: &Loader,
    owner: &str,
    method_name: &str,
    bindings: BTreeMap<String, RubyType>,
    visited: &mut BTreeSet<String>,
) -> Result<Option<RbsCallableOwner>, crate::core::UnknownReason> {
    if !visited.insert(owner.to_string()) {
        return Ok(None);
    }

    if let Some(class) = loader.get_class(owner) {
        if let Some(method) = class.methods.iter().find(|method| {
            method.kind == rbs_parser::MethodKind::Instance && method.name == method_name
        }) {
            return callable_owner_from_method(&class.type_params, method, bindings).map(Some);
        }
        for member in &class.members {
            if let rbs_parser::Member::Include(included) = member {
                if let Some(found) = find_rbs_callable_through_edge(
                    loader,
                    included,
                    method_name,
                    &bindings,
                    visited,
                )? {
                    return Ok(Some(found));
                }
            }
        }
        if let Some(superclass) = &class.superclass {
            if let Some(found) =
                find_rbs_callable_through_edge(loader, superclass, method_name, &bindings, visited)?
            {
                return Ok(Some(found));
            }
        } else if owner != "BasicObject" {
            if let Some(object_params) = declaration_type_parameters(loader, "Object") {
                let object_bindings = declaration_bindings(object_params, &[])?;
                if let Some(found) = find_rbs_callable_owner(
                    loader,
                    "Object",
                    method_name,
                    object_bindings,
                    visited,
                )? {
                    return Ok(Some(found));
                }
            }
        }
    }

    if let Some(module) = loader.get_module(owner) {
        if let Some(method) = module.methods.iter().find(|method| {
            method.kind == rbs_parser::MethodKind::Instance && method.name == method_name
        }) {
            return callable_owner_from_method(&module.type_params, method, bindings).map(Some);
        }
        for member in &module.members {
            if let rbs_parser::Member::Include(included) = member {
                if let Some(found) = find_rbs_callable_through_edge(
                    loader,
                    included,
                    method_name,
                    &bindings,
                    visited,
                )? {
                    return Ok(Some(found));
                }
            }
        }
    }

    Ok(None)
}

fn find_rbs_callable_through_edge(
    loader: &Loader,
    target: &RbsType,
    method_name: &str,
    source_bindings: &BTreeMap<String, RubyType>,
    visited: &mut BTreeSet<String>,
) -> Result<Option<RbsCallableOwner>, crate::core::UnknownReason> {
    let Some(target_name) = rbs_type_to_class_name(target) else {
        return Err(crate::core::UnknownReason::UnsupportedCallable);
    };
    let target_arguments = match target {
        RbsType::Class(_) => Vec::new(),
        RbsType::ClassInstance { args, .. } => {
            let substitutions = source_bindings
                .iter()
                .map(|(name, ruby_type)| (name.clone(), ruby_type.clone()))
                .collect::<HashMap<_, _>>();
            let mut resolved = Vec::with_capacity(args.len());
            for argument in args {
                let ruby_type = substitute_rbs_edge_argument(argument, &substitutions);
                if RubyType::contains_unknown(&ruby_type) {
                    return Err(crate::core::UnknownReason::IncompleteBlockInput);
                }
                resolved.push(ruby_type);
            }
            resolved
        }
        RbsType::Void
        | RbsType::Nil
        | RbsType::Bool
        | RbsType::SelfType
        | RbsType::Instance
        | RbsType::ClassType
        | RbsType::Union(_)
        | RbsType::Intersection(_)
        | RbsType::Optional(_)
        | RbsType::Tuple(_)
        | RbsType::Record(_)
        | RbsType::Proc { .. }
        | RbsType::Literal(_)
        | RbsType::Interface(_)
        | RbsType::TypeVar(_)
        | RbsType::Top
        | RbsType::Bot
        | RbsType::Untyped => return Err(crate::core::UnknownReason::UnsupportedCallable),
    };
    let target_parameters = declaration_type_parameters(loader, &target_name)
        .ok_or(crate::core::UnknownReason::UnsupportedCallable)?;
    let target_bindings = declaration_bindings(target_parameters, &target_arguments)?;
    find_rbs_callable_owner(loader, &target_name, method_name, target_bindings, visited)
}

fn callable_owner_from_method(
    owner_parameters: &[rbs_parser::TypeParam],
    method: &rbs_parser::MethodDecl,
    bindings: BTreeMap<String, RubyType>,
) -> Result<RbsCallableOwner, crate::core::UnknownReason> {
    let owner_parameter_names = owner_parameters
        .iter()
        .map(normalized_type_parameter_name)
        .collect::<Vec<_>>();
    let mut signatures = Vec::new();
    for overload in &method.overloads {
        if overload.block.is_none() {
            continue;
        }
        let method_type_parameters = overload
            .type_params
            .iter()
            .map(normalized_type_parameter_name)
            .collect::<Vec<_>>();
        let signature = callable_signature_from_rbs(
            &owner_parameter_names,
            &method_type_parameters,
            overload,
        )?
        .expect(
            "INVARIANT VIOLATED: an RBS overload lost its checked block during callable conversion. This is a bug because block presence was tested immediately before conversion. Fix: keep the immutable overload and conversion atomic.",
        );
        signatures.push(signature);
    }
    if signatures.is_empty() {
        return Err(crate::core::UnknownReason::UnsupportedCallable);
    }
    Ok(RbsCallableOwner {
        signatures,
        receiver_bindings: bindings.into_iter().collect(),
    })
}
