//! RBS Type Index
//!
//! Provides access to RBS type definitions for built-in Ruby classes.
//! The RBS definitions are embedded in the binary at compile time.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rbs_parser::{Loader, RbsType};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::core::{
    CallableBlockTemplate, CallableSignature, CallableTypeTemplate, FullyQualifiedName, LiteralKey,
    LiteralValue, MethodParamKind, RubyConstant, RubyMethod, RubyType, ShapeExactness, ShapeField,
    ShapeStability, ShapeType,
};
use crate::engine::AnalysisQuery;
use crate::inference::higher_order::{
    callable_signature_from_rbs, prepare_callable_set, PreparedCallableSet,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbsMethodSignature {
    pub parameters: Vec<RbsSignatureParameter>,
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbsSignatureParameter {
    pub name: String,
    pub kind: MethodParamKind,
    pub type_label: String,
}

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

/// Prepare a higher-order call from ordinary file-owned method facts, falling
/// back to the embedded language signatures only when no project signature
/// fact supplies callable evidence.
pub(crate) fn prepare_higher_order_call(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    if let Some(query) = query {
        let method = RubyMethod::new(method_name)
            .map_err(|_| crate::core::UnknownReason::UnsupportedCallable)?;
        let signature_facts = query.resolve_method_signature_facts_for_type(receiver_type, &method);
        let signatures = signature_facts
            .iter()
            .flat_map(|fact| fact.callable_signatures().iter().cloned())
            .collect::<Vec<_>>();
        if !signatures.is_empty() {
            let receiver_parameter_names = signatures[0].receiver_type_parameters.clone();
            if signatures
                .iter()
                .skip(1)
                .any(|signature| signature.receiver_type_parameters != receiver_parameter_names)
            {
                return Err(crate::core::UnknownReason::AmbiguousCallableOverload);
            }
            let receiver_bindings = if receiver_parameter_names.is_empty() {
                Vec::new()
            } else {
                let (_receiver_name, type_arguments) = rbs_receiver_identity(receiver_type)
                    .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
                if receiver_parameter_names.len() != type_arguments.len() {
                    return Err(crate::core::UnknownReason::IncompleteBlockInput);
                }
                receiver_parameter_names
                    .into_iter()
                    .zip(type_arguments)
                    .collect()
            };
            return prepare_callable_set(
                Some(receiver_type),
                &signatures,
                &receiver_bindings,
                argument_types,
            );
        }
    }
    prepare_rbs_higher_order_call(receiver_type, method_name, argument_types)
}

pub(crate) fn prepare_forwarded_higher_order_call(
    query: &AnalysisQuery<'_>,
    receiver_type: Option<&RubyType>,
    implicit_namespace: Option<&FullyQualifiedName>,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let method = RubyMethod::new(method_name)
        .map_err(|_| crate::core::UnknownReason::UnsupportedCallable)?;
    let facts = match (receiver_type, implicit_namespace) {
        (Some(receiver_type), _) => {
            query.resolve_method_signature_facts_for_type(receiver_type, &method)
        }
        (None, Some(namespace)) => query.resolve_method_signature_facts(namespace, &method),
        (None, None) => return Err(crate::core::UnknownReason::IncompleteBlockInput),
    };
    let mut forwarded_targets = Vec::new();
    for fact in facts {
        let Some(forwarded) = fact.forwarded_block_call() else {
            continue;
        };
        let Some(index) = fact
            .param_facts
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    MethodParamKind::Required | MethodParamKind::Optional
                )
            })
            .position(|parameter| parameter.name == forwarded.receiver_parameter)
        else {
            return Err(crate::core::UnknownReason::UnsupportedCallable);
        };
        let receiver = argument_types
            .get(index)
            .cloned()
            .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
        if RubyType::contains_unknown(&receiver) {
            return Err(crate::core::UnknownReason::IncompleteBlockInput);
        }
        forwarded_targets.push((forwarded.method, receiver));
    }
    forwarded_targets
        .sort_by(|left, right| (left.0.as_str(), &left.1).cmp(&(right.0.as_str(), &right.1)));
    forwarded_targets.dedup();
    let [(target_method, target_receiver)] = forwarded_targets.as_slice() else {
        return Err(if forwarded_targets.is_empty() {
            crate::core::UnknownReason::UnsupportedCallable
        } else {
            crate::core::UnknownReason::AmbiguousCallableOverload
        });
    };
    prepare_higher_order_call(Some(query), target_receiver, target_method.as_str(), &[])
}

pub(crate) fn prepare_direct_yield_higher_order_call(
    query: &AnalysisQuery<'_>,
    receiver_type: Option<&RubyType>,
    implicit_namespace: Option<&FullyQualifiedName>,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let method = RubyMethod::new(method_name)
        .map_err(|_| crate::core::UnknownReason::UnsupportedCallable)?;
    let facts = match (receiver_type, implicit_namespace) {
        (Some(receiver_type), _) => {
            query.resolve_method_signature_facts_for_type(receiver_type, &method)
        }
        (None, Some(namespace)) => query.resolve_method_signature_facts(namespace, &method),
        (None, None) => return Err(crate::core::UnknownReason::IncompleteBlockInput),
    };
    let mut block_parameter_sets = Vec::new();
    for fact in facts {
        let Some(direct_yield) = fact.direct_yield_call() else {
            continue;
        };
        let positional = fact
            .param_facts
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    MethodParamKind::Required | MethodParamKind::Optional
                )
            })
            .collect::<Vec<_>>();
        let mut block_parameters = Vec::new();
        for parameter_name in &direct_yield.parameter_names {
            let Some(index) = positional
                .iter()
                .position(|parameter| parameter.name == *parameter_name)
            else {
                return Err(crate::core::UnknownReason::UnsupportedCallable);
            };
            let ruby_type = argument_types
                .get(index)
                .cloned()
                .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
            if RubyType::contains_unknown(&ruby_type) {
                return Err(crate::core::UnknownReason::IncompleteBlockInput);
            }
            block_parameters.push(ruby_type);
        }
        block_parameter_sets.push(block_parameters);
    }
    block_parameter_sets.sort();
    block_parameter_sets.dedup();
    let [block_parameters] = block_parameter_sets.as_slice() else {
        return Err(if block_parameter_sets.is_empty() {
            crate::core::UnknownReason::UnsupportedCallable
        } else {
            crate::core::UnknownReason::AmbiguousCallableOverload
        });
    };
    let output = "BlockResult".to_string();
    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec![output.clone()],
        parameters: Vec::new(),
        block: CallableBlockTemplate {
            parameters: block_parameters
                .iter()
                .cloned()
                .map(CallableTypeTemplate::Concrete)
                .collect(),
            return_type: CallableTypeTemplate::Variable(output.clone()),
            required: true,
        },
        return_type: CallableTypeTemplate::Variable(output),
    };
    prepare_callable_set(None, &[signature], &[], &[])
}

fn rbs_receiver_identity(receiver_type: &RubyType) -> Option<(String, Vec<RubyType>)> {
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

/// Global RBS loader with embedded core types
static RBS_LOADER: Lazy<RwLock<Loader>> = Lazy::new(|| {
    let mut loader = Loader::new();
    if let Err(e) = loader.load_embedded_core() {
        log::warn!("Failed to load embedded RBS core types: {:?}", e);
    }
    log::info!(
        "Loaded {} RBS declarations with {} methods",
        loader.declaration_count(),
        loader.method_count()
    );
    RwLock::new(loader)
});

#[derive(Default)]
struct RbsMethodNameCache {
    instance: HashMap<String, Arc<HashSet<String>>>,
    singleton: HashMap<String, Arc<HashSet<String>>>,
}

/// The embedded RBS environment is immutable after initialization. Cache only
/// declared owners, so this process-wide cache is bounded by the finite set of
/// embedded declarations rather than arbitrary source identifiers.
static RBS_METHOD_NAMES: Lazy<RwLock<RbsMethodNameCache>> =
    Lazy::new(|| RwLock::new(RbsMethodNameCache::default()));

/// Get the return type of a method from RBS definitions
pub fn get_rbs_method_return_type(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
) -> Option<RbsType> {
    let loader = RBS_LOADER.read();
    loader
        .get_method_return_type(class_name, method_name, is_singleton)
        .cloned()
}

pub fn get_rbs_method_signatures(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
) -> Vec<RbsMethodSignature> {
    let loader = RBS_LOADER.read();
    let method = if is_singleton {
        loader.get_singleton_method(class_name, method_name)
    } else {
        loader.get_instance_method(class_name, method_name)
    };
    let Some(method) = method else {
        return Vec::new();
    };

    method
        .overloads
        .iter()
        .map(|overload| RbsMethodSignature {
            parameters: overload
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| RbsSignatureParameter {
                    name: parameter
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("arg{}", index)),
                    kind: match parameter.kind {
                        rbs_parser::ParamKind::Required => MethodParamKind::Required,
                        rbs_parser::ParamKind::Optional => MethodParamKind::Optional,
                        rbs_parser::ParamKind::Rest => MethodParamKind::Rest,
                        rbs_parser::ParamKind::Keyword => MethodParamKind::RequiredKeyword,
                        rbs_parser::ParamKind::KeywordOpt => MethodParamKind::OptionalKeyword,
                        rbs_parser::ParamKind::KeywordRest => MethodParamKind::KeywordRest,
                        rbs_parser::ParamKind::Block => MethodParamKind::Block,
                    },
                    type_label: parameter.r#type.to_string(),
                })
                .collect(),
            return_type: overload.return_type.to_string(),
        })
        .collect()
}

/// Get the return type of a method from RBS, converted to RubyType
pub fn get_rbs_method_return_type_as_ruby_type(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
) -> Option<RubyType> {
    let rbs_type = get_rbs_method_return_type(class_name, method_name, is_singleton)?;
    Some(rbs_type_to_ruby_type(&rbs_type))
}

/// Get the return type of a method from RBS with generic type substitution.
///
/// For example, `Array[Integer]#first` returns `Elem` in RBS, but with
/// `type_args = [Integer]` we substitute `Elem` → `Integer`.
pub fn get_rbs_method_return_type_with_type_args(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
    type_args: &[RubyType],
) -> Option<RubyType> {
    let loader = RBS_LOADER.read();
    let rbs_type = loader
        .get_method_return_type(class_name, method_name, is_singleton)?
        .clone();

    // Build substitution map from class type_params to actual type_args
    let substitutions = if let Some(class) = loader.get_class(class_name) {
        build_substitution_map(&class.type_params, type_args)
    } else if let Some(module) = loader.get_module(class_name) {
        build_substitution_map(&module.type_params, type_args)
    } else {
        std::collections::HashMap::new()
    };

    if substitutions.is_empty() {
        Some(rbs_type_to_ruby_type(&rbs_type))
    } else {
        Some(rbs_type_to_ruby_type_with_substitutions(
            &rbs_type,
            &substitutions,
        ))
    }
}

/// Build a map from type parameter names to concrete RubyTypes.
/// Handles type param names that include modifiers like "unchecked out Elem" → "Elem".
fn build_substitution_map(
    type_params: &[rbs_parser::TypeParam],
    type_args: &[RubyType],
) -> std::collections::HashMap<String, RubyType> {
    type_params
        .iter()
        .zip(type_args.iter())
        .map(|(param, arg)| {
            // Strip modifiers: "unchecked out Elem" → "Elem"
            let name = param
                .name
                .rsplit_once(' ')
                .map(|(_, name)| name)
                .unwrap_or(&param.name);
            (name.to_string(), arg.clone())
        })
        .collect()
}

/// Convert an RbsType to a RubyType, substituting type variables with concrete types
fn rbs_type_to_ruby_type_with_substitutions(
    rbs_type: &RbsType,
    substitutions: &std::collections::HashMap<String, RubyType>,
) -> RubyType {
    match rbs_type {
        RbsType::TypeVar(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or(RubyType::Unknown),
        // The RBS parser sometimes represents type variables as Class("Elem")
        // instead of TypeVar("Elem"). Check substitutions for class names too.
        RbsType::Class(name) => {
            if let Some(substituted) = substitutions.get(name) {
                return substituted.clone();
            }
            class_name_to_ruby_type(name)
        }
        // For compound types, recurse with substitutions
        RbsType::Union(types) => RubyType::union(
            types
                .iter()
                .map(|t| rbs_type_to_ruby_type_with_substitutions(t, substitutions)),
        ),
        RbsType::Optional(inner) => {
            let inner_type = rbs_type_to_ruby_type_with_substitutions(inner, substitutions);
            RubyType::optional(inner_type)
        }
        RbsType::ClassInstance { name, args } => {
            let clean_name = name.strip_prefix("::").unwrap_or(name);
            if args.is_empty() {
                if let Some(substituted) = substitutions.get(clean_name) {
                    return substituted.clone();
                }
            }
            match clean_name {
                "Array" => {
                    let element_type = match args.as_slice() {
                        [element] => {
                            rbs_type_to_ruby_type_with_substitutions(element, substitutions)
                        }
                        [] | [_, _, ..] => RubyType::Unknown,
                    };
                    RubyType::Array(vec![element_type])
                }
                "Hash" => {
                    let (key_type, value_type) = match args.as_slice() {
                        [key, value] => (
                            rbs_type_to_ruby_type_with_substitutions(key, substitutions),
                            rbs_type_to_ruby_type_with_substitutions(value, substitutions),
                        ),
                        [] | [_] | [_, _, _, ..] => (RubyType::Unknown, RubyType::Unknown),
                    };
                    RubyType::Hash(vec![key_type], vec![value_type])
                }
                _ => class_name_to_ruby_type(clean_name),
            }
        }
        RbsType::Record(fields) => rbs_record_to_ruby_type(fields, |field_type| {
            rbs_type_to_ruby_type_with_substitutions(field_type, substitutions)
        }),
        // For all other types, fall back to the non-substitution version
        _ => rbs_type_to_ruby_type(rbs_type),
    }
}

/// Convert a class name string to RubyType, handling special cases and leading ::
fn class_name_to_ruby_type(name: &str) -> RubyType {
    // Strip leading :: for absolute references
    let clean_name = name.strip_prefix("::").unwrap_or(name);

    // Handle special cases
    match clean_name {
        "String" => RubyType::string(),
        "Integer" => RubyType::integer(),
        "Float" => RubyType::float(),
        "Symbol" => RubyType::symbol(),
        "TrueClass" => RubyType::true_class(),
        "FalseClass" => RubyType::false_class(),
        "NilClass" => RubyType::nil_class(),
        _ => clean_name
            .split("::")
            .map(RubyConstant::new)
            .collect::<Result<Vec<_>, _>>()
            .map(FullyQualifiedName::constant)
            .map(RubyType::Class)
            .unwrap_or(RubyType::Unknown),
    }
}

/// Convert an RbsType to a RubyType
pub fn rbs_type_to_ruby_type(rbs_type: &RbsType) -> RubyType {
    match rbs_type {
        RbsType::Void => RubyType::nil_class(),
        RbsType::Nil => RubyType::nil_class(),
        RbsType::Bool => RubyType::boolean(),
        RbsType::Top | RbsType::Bot | RbsType::Untyped => RubyType::Unknown,
        RbsType::SelfType => RubyType::Unknown, // TODO: Track self type in context
        RbsType::Instance => RubyType::Unknown, // TODO: Track instance type in context
        RbsType::Class(name) => class_name_to_ruby_type(name),
        RbsType::ClassInstance { name, args } => {
            // Strip leading :: from name for matching
            let clean_name = name.strip_prefix("::").unwrap_or(name);
            // Handle generic types like Array[String]
            match clean_name {
                "Array" => {
                    let element_type = match args.as_slice() {
                        [element] => rbs_type_to_ruby_type(element),
                        [] | [_, _, ..] => RubyType::Unknown,
                    };
                    RubyType::Array(vec![element_type])
                }
                "Hash" => {
                    let (key_type, value_type) = match args.as_slice() {
                        [key, value] => (rbs_type_to_ruby_type(key), rbs_type_to_ruby_type(value)),
                        [] | [_] | [_, _, _, ..] => (RubyType::Unknown, RubyType::Unknown),
                    };
                    RubyType::Hash(vec![key_type], vec![value_type])
                }
                _ => class_name_to_ruby_type(clean_name),
            }
        }
        RbsType::ClassType => {
            // The `class` type - represents a class object
            RubyType::Unknown
        }
        RbsType::Union(types) => RubyType::union(types.iter().map(rbs_type_to_ruby_type)),
        RbsType::Intersection(_) => RubyType::Unknown,
        RbsType::Optional(inner) => {
            let inner_type = rbs_type_to_ruby_type(inner);
            RubyType::optional(inner_type)
        }
        RbsType::Tuple(types) => RubyType::Array(RubyType::canonical_union_members(
            types.iter().map(rbs_type_to_ruby_type),
        )),
        RbsType::Record(fields) => rbs_record_to_ruby_type(fields, rbs_type_to_ruby_type),
        RbsType::Proc { .. } => {
            // Proc types - just use Proc class for now
            if let Ok(constant) = RubyConstant::new("Proc") {
                RubyType::Class(FullyQualifiedName::constant(vec![constant]))
            } else {
                RubyType::Unknown
            }
        }
        RbsType::Literal(rbs_parser::Literal::String(value)) => {
            RubyType::Literal(Box::new(LiteralValue::string(value.clone())))
        }
        RbsType::Literal(rbs_parser::Literal::Symbol(value)) => {
            RubyType::Literal(Box::new(LiteralValue::symbol(value.clone())))
        }
        RbsType::Literal(rbs_parser::Literal::Integer(_)) => RubyType::integer(),
        RbsType::Literal(rbs_parser::Literal::True) => RubyType::true_class(),
        RbsType::Literal(rbs_parser::Literal::False) => RubyType::false_class(),
        RbsType::Interface(name) => {
            // Interface types
            if let Ok(constant) = RubyConstant::new(name) {
                RubyType::Class(FullyQualifiedName::constant(vec![constant]))
            } else {
                RubyType::Unknown
            }
        }
        RbsType::TypeVar(_) => {
            // Type variables like T - can't resolve without context
            RubyType::Unknown
        }
    }
}

fn rbs_record_to_ruby_type(
    fields: &[rbs_parser::RecordField],
    mut convert: impl FnMut(&RbsType) -> RubyType,
) -> RubyType {
    let fields = fields.iter().map(|field| {
        let key = LiteralKey::symbol(field.name.clone());
        let value = convert(&field.r#type);
        if field.optional {
            ShapeField::optional(key, value)
        } else {
            ShapeField::required(key, value)
        }
    });
    ShapeType::try_new(
        fields,
        None,
        ShapeExactness::Exact,
        ShapeStability::TrackedMutable,
    )
    .map(|shape| RubyType::Shape(Box::new(shape)))
    .unwrap_or(RubyType::Unknown)
}

/// Check if a class exists in RBS definitions
pub fn has_rbs_class(class_name: &str) -> bool {
    let loader = RBS_LOADER.read();
    loader.get_class(class_name).is_some()
}

/// Method info for completion
#[derive(Debug, Clone)]
pub struct RbsMethodInfo {
    pub name: String,
    pub return_type: Option<RubyType>,
    pub is_singleton: bool,
    pub params: Vec<String>,
}

/// Get all methods for a class from RBS definitions, including inherited methods
/// from the ancestor chain (superclass + included modules).
pub fn get_rbs_class_methods(class_name: &str, include_singleton: bool) -> Vec<RbsMethodInfo> {
    let loader = RBS_LOADER.read();
    let mut methods = Vec::new();
    let mut seen_methods = HashSet::new();
    let mut visited = HashSet::new();

    collect_rbs_methods_recursive(
        &loader,
        class_name,
        include_singleton,
        &mut methods,
        &mut seen_methods,
        &mut visited,
    );

    methods
}

pub fn rbs_class_method_exists(
    class_name: &str,
    method_name: &str,
    include_singleton: bool,
) -> bool {
    rbs_method_name_catalog(class_name, include_singleton).contains(method_name)
}

fn rbs_method_name_catalog(class_name: &str, include_singleton: bool) -> Arc<HashSet<String>> {
    {
        let cache = RBS_METHOD_NAMES.read();
        let catalogs = if include_singleton {
            &cache.singleton
        } else {
            &cache.instance
        };
        if let Some(catalog) = catalogs.get(class_name) {
            return Arc::clone(catalog);
        }
    }

    // Hold the write lock through construction so concurrent project indexers
    // single-flight the first catalog build instead of duplicating the same
    // recursive RBS traversal.
    let mut cache = RBS_METHOD_NAMES.write();
    let catalogs = if include_singleton {
        &mut cache.singleton
    } else {
        &mut cache.instance
    };
    if let Some(catalog) = catalogs.get(class_name) {
        return Arc::clone(catalog);
    }

    let loader = RBS_LOADER.read();
    let owner_is_declared =
        loader.get_class(class_name).is_some() || loader.get_module(class_name).is_some();
    if !owner_is_declared {
        return Arc::new(HashSet::new());
    }

    let mut method_names = HashSet::new();
    let mut visited = HashSet::new();
    collect_rbs_method_names_recursive(
        &loader,
        class_name,
        include_singleton,
        &mut method_names,
        &mut visited,
    );

    let catalog = Arc::new(method_names);
    catalogs.insert(class_name.to_string(), Arc::clone(&catalog));
    catalog
}

/// Extract a class/module name from an RbsType (used for ancestor resolution)
fn rbs_type_to_class_name(rbs_type: &RbsType) -> Option<String> {
    match rbs_type {
        RbsType::Class(name) => Some(name.strip_prefix("::").unwrap_or(name).to_string()),
        RbsType::ClassInstance { name, .. } => {
            Some(name.strip_prefix("::").unwrap_or(name).to_string())
        }
        _ => None,
    }
}

fn substitute_rbs_edge_argument(
    rbs_type: &RbsType,
    substitutions: &HashMap<String, RubyType>,
) -> RubyType {
    match rbs_type {
        RbsType::Class(name) | RbsType::TypeVar(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| rbs_type_to_ruby_type_with_substitutions(rbs_type, substitutions)),
        RbsType::ClassInstance { name, args } if args.is_empty() => substitutions
            .get(name.strip_prefix("::").unwrap_or(name))
            .cloned()
            .unwrap_or_else(|| rbs_type_to_ruby_type_with_substitutions(rbs_type, substitutions)),
        RbsType::Void
        | RbsType::Nil
        | RbsType::Bool
        | RbsType::SelfType
        | RbsType::Instance
        | RbsType::ClassType
        | RbsType::ClassInstance { .. }
        | RbsType::Union(_)
        | RbsType::Intersection(_)
        | RbsType::Optional(_)
        | RbsType::Tuple(_)
        | RbsType::Record(_)
        | RbsType::Proc { .. }
        | RbsType::Literal(_)
        | RbsType::Interface(_)
        | RbsType::Top
        | RbsType::Bot
        | RbsType::Untyped => rbs_type_to_ruby_type_with_substitutions(rbs_type, substitutions),
    }
}

/// Recursively collect methods from a class/module and its ancestors
fn collect_rbs_methods_recursive(
    loader: &Loader,
    class_name: &str,
    include_singleton: bool,
    methods: &mut Vec<RbsMethodInfo>,
    seen_methods: &mut std::collections::HashSet<String>,
    visited: &mut std::collections::HashSet<String>,
) {
    // Prevent infinite recursion from circular inheritance
    if !visited.insert(class_name.to_string()) {
        return;
    }

    // Collect methods from this class
    if let Some(class) = loader.get_class(class_name) {
        collect_methods_from_decl(&class.methods, include_singleton, methods, seen_methods);

        // Process aliases (e.g., `alias object_id __id__`)
        collect_aliases_from_members(
            &class.members,
            &class.methods,
            include_singleton,
            methods,
            seen_methods,
        );

        // Walk included modules (instance methods become available)
        for member in &class.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_methods_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        methods,
                        seen_methods,
                        visited,
                    );
                }
            }
        }

        // Walk superclass
        if let Some(superclass) = &class.superclass {
            if let Some(parent_name) = rbs_type_to_class_name(superclass) {
                collect_rbs_methods_recursive(
                    loader,
                    &parent_name,
                    include_singleton,
                    methods,
                    seen_methods,
                    visited,
                );
            }
        } else if class_name != "BasicObject" {
            // Implicit superclass is Object (unless we're BasicObject)
            collect_rbs_methods_recursive(
                loader,
                "Object",
                include_singleton,
                methods,
                seen_methods,
                visited,
            );
        }
    }

    // Also check modules (for when class_name is a module, or for mixed-in methods)
    if let Some(module) = loader.get_module(class_name) {
        collect_methods_from_decl(&module.methods, include_singleton, methods, seen_methods);

        // Process aliases in module
        collect_aliases_from_members(
            &module.members,
            &module.methods,
            include_singleton,
            methods,
            seen_methods,
        );

        // Walk included modules within this module
        for member in &module.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_methods_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        methods,
                        seen_methods,
                        visited,
                    );
                }
            }
        }
    }
}

fn collect_rbs_method_names_recursive(
    loader: &Loader,
    class_name: &str,
    include_singleton: bool,
    method_names: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(class_name.to_string()) {
        return;
    }

    if let Some(class) = loader.get_class(class_name) {
        collect_method_names_from_decl(&class.methods, include_singleton, method_names);
        collect_alias_names_from_members(&class.members, include_singleton, method_names);

        for member in &class.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_method_names_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        method_names,
                        visited,
                    );
                }
            }
        }

        if let Some(superclass) = &class.superclass {
            if let Some(parent_name) = rbs_type_to_class_name(superclass) {
                collect_rbs_method_names_recursive(
                    loader,
                    &parent_name,
                    include_singleton,
                    method_names,
                    visited,
                );
            }
        } else if class_name != "BasicObject" {
            collect_rbs_method_names_recursive(
                loader,
                "Object",
                include_singleton,
                method_names,
                visited,
            );
        }
    }

    if let Some(module) = loader.get_module(class_name) {
        collect_method_names_from_decl(&module.methods, include_singleton, method_names);
        collect_alias_names_from_members(&module.members, include_singleton, method_names);

        for member in &module.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_method_names_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        method_names,
                        visited,
                    );
                }
            }
        }
    }
}

fn collect_method_names_from_decl(
    method_decls: &[rbs_parser::MethodDecl],
    include_singleton: bool,
    method_names: &mut HashSet<String>,
) {
    for method in method_decls {
        let is_singleton = method.kind == rbs_parser::MethodKind::Singleton;
        if !is_singleton || include_singleton {
            method_names.insert(method.name.clone());
        }
    }
}

fn collect_alias_names_from_members(
    members: &[rbs_parser::Member],
    include_singleton: bool,
    method_names: &mut HashSet<String>,
) {
    for member in members {
        if let rbs_parser::Member::Alias(alias) = member {
            if !alias.is_singleton || include_singleton {
                method_names.insert(alias.new_name.clone());
            }
        }
    }
}

/// Collect methods from a list of MethodDecl into the methods vec, skipping duplicates
fn collect_methods_from_decl(
    method_decls: &[rbs_parser::MethodDecl],
    include_singleton: bool,
    methods: &mut Vec<RbsMethodInfo>,
    seen_methods: &mut std::collections::HashSet<String>,
) {
    for method in method_decls {
        let is_singleton = method.kind == rbs_parser::MethodKind::Singleton;

        if is_singleton && !include_singleton {
            continue;
        }

        if !seen_methods.insert(method.name.clone()) {
            continue; // Already seen — subclass method takes priority
        }

        let params: Vec<String> = method
            .overloads
            .first()
            .map(|o| {
                o.params
                    .iter()
                    .map(|p| p.name.clone().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();

        let return_type = method.return_type().map(rbs_type_to_ruby_type);

        methods.push(RbsMethodInfo {
            name: method.name.clone(),
            return_type,
            is_singleton,
            params,
        });
    }
}

/// Process alias declarations from class/module members.
/// For `alias object_id __id__`, creates a method entry for `object_id`
/// with the same signature as `__id__`.
fn collect_aliases_from_members(
    members: &[rbs_parser::Member],
    method_decls: &[rbs_parser::MethodDecl],
    include_singleton: bool,
    methods: &mut Vec<RbsMethodInfo>,
    seen_methods: &mut std::collections::HashSet<String>,
) {
    for member in members {
        if let rbs_parser::Member::Alias(alias) = member {
            if alias.is_singleton && !include_singleton {
                continue;
            }
            if !seen_methods.insert(alias.new_name.clone()) {
                continue;
            }

            // Look up the target method to copy its signature
            let target = method_decls.iter().find(|m| m.name == alias.old_name);
            let (return_type, params) = if let Some(target_method) = target {
                let rt = target_method.return_type().map(rbs_type_to_ruby_type);
                let ps: Vec<String> = target_method
                    .overloads
                    .first()
                    .map(|o| {
                        o.params
                            .iter()
                            .map(|p| p.name.clone().unwrap_or_default())
                            .collect()
                    })
                    .unwrap_or_default();
                (rt, ps)
            } else {
                (None, vec![])
            };

            methods.push(RbsMethodInfo {
                name: alias.new_name.clone(),
                return_type,
                is_singleton: alias.is_singleton,
                params,
            });
        }
    }
}

/// Get the number of loaded RBS declarations
pub fn rbs_declaration_count() -> usize {
    let loader = RBS_LOADER.read();
    loader.declaration_count()
}

/// Get the number of loaded RBS methods
pub fn rbs_method_count() -> usize {
    let loader = RBS_LOADER.read();
    loader.method_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbs_loader_initialized() {
        let count = rbs_declaration_count();
        assert!(count > 0, "RBS loader should have declarations");
        println!("Loaded {} RBS declarations", count);
    }

    #[test]
    fn test_string_length_return_type() {
        let return_type = get_rbs_method_return_type("String", "length", false);
        assert!(
            return_type.is_some(),
            "String#length should have a return type"
        );
        if let Some(RbsType::Class(name)) = return_type {
            assert_eq!(name, "Integer");
        } else {
            panic!("Expected Integer return type, got {:?}", return_type);
        }
    }

    #[test]
    fn test_string_upcase_return_type() {
        let return_type = get_rbs_method_return_type("String", "upcase", false);
        assert!(
            return_type.is_some(),
            "String#upcase should have a return type"
        );
        // String#upcase returns `self?` in RBS (optional self type)
        // This is correct - it returns the same string
        println!("String#upcase return type: {:?}", return_type);
    }

    #[test]
    fn test_string_downcase_return_type() {
        let return_type = get_rbs_method_return_type("String", "downcase", false);
        assert!(
            return_type.is_some(),
            "String#downcase should have a return type"
        );
        println!("String#downcase return type: {:?}", return_type);
    }

    #[test]
    fn intersection_type_does_not_select_an_arbitrary_member() {
        let rbs_type = RbsType::Intersection(vec![
            RbsType::Class("String".to_string()),
            RbsType::Class("Integer".to_string()),
        ]);

        assert_eq!(rbs_type_to_ruby_type(&rbs_type), RubyType::Unknown);
    }

    #[test]
    fn union_with_untyped_member_remains_unknown() {
        let rbs_type = RbsType::Union(vec![RbsType::Class("String".to_string()), RbsType::Untyped]);

        assert_eq!(rbs_type_to_ruby_type(&rbs_type), RubyType::Unknown);
    }

    #[test]
    fn test_integer_to_s_return_type() {
        let return_type = get_rbs_method_return_type("Integer", "to_s", false);
        assert!(
            return_type.is_some(),
            "Integer#to_s should have a return type"
        );
        if let Some(RbsType::Class(name)) = return_type {
            assert_eq!(name, "String");
        } else {
            panic!("Expected String return type, got {:?}", return_type);
        }
    }

    #[test]
    fn test_array_first_return_type() {
        let return_type = get_rbs_method_return_type("Array", "first", false);
        assert!(
            return_type.is_some(),
            "Array#first should have a return type"
        );
        println!("Array#first return type: {:?}", return_type);
    }

    #[test]
    fn test_has_string_class() {
        assert!(has_rbs_class("String"), "Should have String class");
        assert!(has_rbs_class("Integer"), "Should have Integer class");
        assert!(has_rbs_class("Array"), "Should have Array class");
        assert!(has_rbs_class("Hash"), "Should have Hash class");
    }

    #[test]
    fn test_class_has_new_method() {
        assert!(has_rbs_class("Class"), "Should have Class class in RBS");
        let methods = get_rbs_class_methods("Class", false);
        let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(
            method_names.contains(&"new"),
            "Class should have 'new' instance method. Found: {:?}",
            method_names
        );
    }

    #[test]
    fn test_nonexistent_method() {
        let return_type = get_rbs_method_return_type("String", "nonexistent_method_xyz", false);
        assert!(return_type.is_none(), "Should not find nonexistent method");
    }

    #[test]
    fn test_string_chars_return_type() {
        let return_type = get_rbs_method_return_type("String", "chars", false);
        println!("String#chars return type: {:?}", return_type);
        assert!(
            return_type.is_some(),
            "String#chars should have a return type"
        );

        // Also test the RubyType conversion
        let ruby_type = get_rbs_method_return_type_as_ruby_type("String", "chars", false);
        println!("String#chars as RubyType: {:?}", ruby_type);
    }

    #[test]
    fn test_nil_class_inherits_object_methods() {
        let methods = get_rbs_class_methods("NilClass", false);
        let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();

        // NilClass's own methods
        assert!(method_names.contains(&"nil?"), "Should have NilClass#nil?");
        assert!(method_names.contains(&"to_s"), "Should have NilClass#to_s");
        assert!(
            method_names.contains(&"inspect"),
            "Should have NilClass#inspect"
        );

        // Inherited from Object/Kernel/BasicObject
        assert!(
            method_names.contains(&"class"),
            "Should have Object#class (inherited)"
        );
        assert!(
            method_names.contains(&"is_a?"),
            "Should have Kernel#is_a? (inherited)"
        );
        assert!(
            method_names.contains(&"freeze"),
            "Should have Object#freeze (inherited)"
        );
        assert!(
            method_names.contains(&"respond_to?"),
            "Should have Kernel#respond_to? (inherited)"
        );
        assert!(
            method_names.contains(&"tap"),
            "Should have Kernel#tap (inherited)"
        );
        assert!(
            method_names.contains(&"public_send"),
            "Should have Kernel#public_send (inherited)"
        );
        // Aliases should also be resolved
        assert!(
            method_names.contains(&"object_id"),
            "Should have Kernel#object_id (alias of __id__)"
        );
    }

    #[test]
    fn test_string_inherits_object_methods() {
        let methods = get_rbs_class_methods("String", false);
        let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();

        // String's own methods
        assert!(
            method_names.contains(&"upcase"),
            "Should have String#upcase"
        );
        assert!(
            method_names.contains(&"length"),
            "Should have String#length"
        );

        // Inherited from Object/Kernel
        assert!(
            method_names.contains(&"class"),
            "Should have Object#class (inherited)"
        );
        assert!(
            method_names.contains(&"is_a?"),
            "Should have Kernel#is_a? (inherited)"
        );
        assert!(
            method_names.contains(&"tap"),
            "Should have Kernel#tap (inherited)"
        );
    }

    #[test]
    fn rbs_method_catalog_is_shared_and_preserves_inherited_aliases() {
        let first = rbs_method_name_catalog("String", false);
        let second = rbs_method_name_catalog("String", false);

        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "INVARIANT VIOLATED: repeated immutable RBS catalog queries rebuilt String. \
             This is a bug because the embedded RBS environment cannot change at runtime. \
             Fix: share one catalog per declared RBS owner and singleton mode."
        );
        assert!(
            first.contains("tap"),
            "INVARIANT VIOLATED: cached String methods lost inherited Kernel#tap. \
             This is a bug because caching must preserve the complete RBS ancestor lookup. \
             Fix: construct the cache entry through the ordinary recursive collector."
        );
        assert!(
            first.contains("object_id"),
            "INVARIANT VIOLATED: cached String methods lost the Kernel#object_id alias. \
             This is a bug because cached and uncached RBS lookup must be semantically identical. \
             Fix: retain alias collection when constructing a cache entry."
        );
    }

    #[test]
    fn test_generic_type_substitution_array_first() {
        let type_args = vec![RubyType::integer()];
        let result = get_rbs_method_return_type_with_type_args("Array", "first", false, &type_args);
        assert!(
            result.is_some(),
            "Array#first with type_args should return a type"
        );
        let rt = result.unwrap();
        assert_eq!(
            rt,
            RubyType::integer(),
            "Array[Integer]#first should return Integer"
        );
    }

    #[test]
    fn test_generic_type_substitution_hash_keys() {
        let type_args = vec![RubyType::symbol(), RubyType::string()];
        let result = get_rbs_method_return_type_with_type_args("Hash", "keys", false, &type_args);
        assert!(result.is_some(), "Hash#keys should return a type");
        let rt = result.unwrap();
        // Hash[Symbol, String]#keys should return Array[Symbol]
        assert_eq!(
            rt,
            RubyType::Array(vec![RubyType::symbol()]),
            "Hash[Symbol, String]#keys should return Array[Symbol]"
        );
    }

    #[test]
    fn array_map_callable_uses_the_generic_enumerable_include() {
        let prepared =
            prepare_rbs_higher_order_call(&RubyType::array_of(RubyType::integer()), "map", &[])
                .expect("Array#map must resolve through Enumerable[Elem]");
        assert_eq!(prepared.block_parameter_types(), &[RubyType::integer()]);
        assert_eq!(
            prepared.finish(&RubyType::string()).proven_type(),
            Some(&RubyType::array_of(RubyType::string()))
        );
    }

    #[test]
    fn array_each_callable_binds_the_receiver_element() {
        let prepared =
            prepare_rbs_higher_order_call(&RubyType::array_of(RubyType::integer()), "each", &[])
                .expect("Array#each must bind its receiver element generic");
        assert_eq!(prepared.block_parameter_types(), &[RubyType::integer()]);
    }

    #[test]
    fn enumerable_filter_map_callable_subtracts_falsey_results() {
        let prepared = prepare_rbs_higher_order_call(
            &RubyType::array_of(RubyType::integer()),
            "filter_map",
            &[],
        )
        .expect("Array#filter_map must resolve through Enumerable[Elem]");
        let block_result = RubyType::union([RubyType::nil_class(), RubyType::string()]);
        assert_eq!(
            prepared.finish(&block_result).proven_type(),
            Some(&RubyType::array_of(RubyType::string()))
        );
    }

    #[test]
    fn enumerable_each_with_object_binds_the_accumulator_argument() {
        let accumulator = RubyType::hash_of(RubyType::symbol(), RubyType::integer());
        let prepared = prepare_rbs_higher_order_call(
            &RubyType::array_of(RubyType::integer()),
            "each_with_object",
            std::slice::from_ref(&accumulator),
        )
        .expect("Array#each_with_object must resolve through Enumerable[Elem]");
        assert_eq!(
            prepared.block_parameter_types(),
            &[RubyType::integer(), accumulator.clone()]
        );
        assert_eq!(
            prepared.finish(&RubyType::integer()).proven_type(),
            Some(&accumulator)
        );
    }
}
