//! Which overload of a bundled signature a call selects.
//!
//! A signature may declare several overloads that return different types. The
//! argument count, the block, and the proven argument types of a call decide
//! which of them can apply. When overloads with different returns are left
//! standing, the call has no proven type: the first overload is never assumed.

use rbs_parser::{Loader, MethodParam, MethodType, ParamKind, RbsType, TypeParam};
use std::collections::HashMap;

use super::catalog::rbs_method_name_catalog;
use super::conversion::{
    rbs_type_to_class_name, rbs_type_to_ruby_type, rbs_type_to_ruby_type_with_substitutions,
};
use super::RBS_LOADER;
use crate::core::{RubyType, UnknownReason};

/// Alias and superclass chains longer than this are left undecided.
const MAX_TYPE_DEPTH: usize = 16;

/// What a call site proves about the arguments it passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallArguments {
    /// Positional argument types in order; `Unknown` where unproven.
    pub positional: Vec<RubyType>,
    /// Whether a block is attached; `None` when the call site does not decide.
    pub block: Option<bool>,
}

/// The return of `method_name` for the call `arguments` describes.
///
/// `None` means no bundled signature exists. `Err` means one exists but the
/// call does not prove a single return. `arguments` is `None` for a call
/// whose site is not visible, or whose arguments cannot be matched to
/// parameters one by one; every overload then stays standing.
pub(crate) fn rbs_call_return(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
    type_args: &[RubyType],
    arguments: Option<&CallArguments>,
) -> Option<Result<RubyType, UnknownReason>> {
    let loader = RBS_LOADER.read();
    let method = if is_singleton {
        loader.get_singleton_method(class_name, method_name)
    } else {
        loader.get_instance_method(class_name, method_name)
    }?;
    let class_params = class_type_params(&loader, class_name);
    let first = method.overloads.first()?;
    let standing = if returns_agree(&method.overloads) {
        vec![first]
    } else if let Some(arguments) = arguments {
        standing_overloads(&loader, &method.overloads, class_params, arguments)
    } else {
        method.overloads.iter().collect()
    };
    let mut returns: Vec<RubyType> = Vec::new();
    for overload in standing {
        let return_type = overload_return(overload, class_params, type_args);
        if !returns.contains(&return_type) {
            returns.push(return_type);
        }
    }
    let only = returns.pop();
    Some(match (only, returns.is_empty()) {
        (Some(return_type), true) => Ok(return_type),
        (Some(_), false) => Err(UnknownReason::AmbiguousCallableOverload),
        // No overload accepts the call: the engine must not answer it from
        // the signature either.
        (None, _) => Err(UnknownReason::AmbiguousCallableOverload),
    })
}

/// Whether the arguments of a call decide what `method_name` returns: its
/// bundled overloads return different types.
pub(crate) fn rbs_return_depends_on_arguments(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
) -> bool {
    let loader = RBS_LOADER.read();
    let method = if is_singleton {
        loader.get_singleton_method(class_name, method_name)
    } else {
        loader.get_instance_method(class_name, method_name)
    };
    method.is_some_and(|method| !returns_agree(&method.overloads))
}

fn returns_agree(overloads: &[MethodType]) -> bool {
    overloads
        .windows(2)
        .all(|pair| pair[0].return_type == pair[1].return_type)
}

fn class_type_params<'a>(loader: &'a Loader, class_name: &str) -> &'a [TypeParam] {
    if let Some(class) = loader.get_class(class_name) {
        &class.type_params
    } else if let Some(module) = loader.get_module(class_name) {
        &module.type_params
    } else {
        &[]
    }
}

/// A type parameter's name without modifiers: "unchecked out Elem" is "Elem".
fn type_param_name(param: &TypeParam) -> &str {
    param
        .name
        .rsplit_once(' ')
        .map(|(_, name)| name)
        .unwrap_or(&param.name)
}

/// The overload's return with the class's type parameters bound to
/// `type_args`. A parameter without an argument, and every parameter of the
/// method itself, is `Unknown`: the parser may spell it like a class name,
/// which must not leak as a concrete class.
fn overload_return(
    overload: &MethodType,
    class_params: &[TypeParam],
    type_args: &[RubyType],
) -> RubyType {
    let mut substitutions: HashMap<String, RubyType> = overload
        .type_params
        .iter()
        .map(|param| (type_param_name(param).to_string(), RubyType::Unknown))
        .collect();
    for (index, param) in class_params.iter().enumerate() {
        let argument = type_args.get(index).unwrap_or(&RubyType::Unknown);
        substitutions.insert(type_param_name(param).to_string(), argument.clone());
    }
    if substitutions.is_empty() {
        rbs_type_to_ruby_type(&overload.return_type)
    } else {
        rbs_type_to_ruby_type_with_substitutions(&overload.return_type, &substitutions)
    }
}

/// Whether one overload, parameter, or type accepts what the call passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    /// Proven to accept it.
    Applies,
    /// Neither proven nor ruled out.
    Undecided,
    /// Proven not to accept it.
    Excluded,
}

impl Fit {
    /// Both must hold.
    fn and(self, other: Fit) -> Fit {
        match (self, other) {
            (Fit::Excluded, _) | (_, Fit::Excluded) => Fit::Excluded,
            (Fit::Undecided, _) | (_, Fit::Undecided) => Fit::Undecided,
            (Fit::Applies, Fit::Applies) => Fit::Applies,
        }
    }

    /// The value is accepted when any alternative accepts it.
    fn any(fits: impl Iterator<Item = Fit>) -> Fit {
        let mut result = Fit::Excluded;
        for fit in fits {
            match fit {
                Fit::Applies => return Fit::Applies,
                Fit::Undecided => result = Fit::Undecided,
                Fit::Excluded => {}
            }
        }
        result
    }

    /// Every alternative of the value must be accepted; a value with some
    /// accepted and some excluded alternatives is undecided.
    fn every_alternative(fits: impl Iterator<Item = Fit>) -> Fit {
        let fits: Vec<Fit> = fits.collect();
        if fits.iter().all(|fit| *fit == Fit::Applies) {
            Fit::Applies
        } else if fits.iter().all(|fit| *fit == Fit::Excluded) {
            Fit::Excluded
        } else {
            Fit::Undecided
        }
    }
}

/// The overloads the call can still select, in declaration order. Overloads
/// are tried in order, so one that is proven to apply hides every later one.
fn standing_overloads<'a>(
    loader: &Loader,
    overloads: &'a [MethodType],
    class_params: &[TypeParam],
    arguments: &CallArguments,
) -> Vec<&'a MethodType> {
    let some_overload_takes_a_block = overloads.iter().any(|overload| overload.block.is_some());
    let mut standing = Vec::new();
    for overload in overloads {
        let matcher = Matcher {
            loader,
            class_params,
            method_params: &overload.type_params,
        };
        let fit = matcher
            .block_fit(overload, arguments.block, some_overload_takes_a_block)
            .and(matcher.positional_fit(overload, &arguments.positional));
        match fit {
            Fit::Excluded => {}
            Fit::Undecided => standing.push(overload),
            Fit::Applies => {
                standing.push(overload);
                break;
            }
        }
    }
    standing
}

struct Matcher<'a> {
    loader: &'a Loader,
    class_params: &'a [TypeParam],
    method_params: &'a [TypeParam],
}

impl Matcher<'_> {
    /// A block passed to a method none of whose overloads take one is
    /// ignored by Ruby, so blocks select only among overloads that differ
    /// in taking one.
    fn block_fit(&self, overload: &MethodType, block: Option<bool>, block_selects: bool) -> Fit {
        if !block_selects {
            return Fit::Applies;
        }
        match (&overload.block, block) {
            (None, Some(false)) | (Some(_), Some(true)) => Fit::Applies,
            (None, Some(true)) => Fit::Excluded,
            (Some(signature), Some(false)) if signature.required => Fit::Excluded,
            (Some(_), Some(false)) => Fit::Applies,
            (Some(signature), None) if !signature.required => Fit::Applies,
            (None, None) | (Some(_), None) => Fit::Undecided,
        }
    }

    fn positional_fit(&self, overload: &MethodType, arguments: &[RubyType]) -> Fit {
        let positional: Vec<&MethodParam> = overload
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.kind,
                    ParamKind::Required | ParamKind::Optional | ParamKind::Rest
                )
            })
            .collect();
        let takes_keywords = overload.params.iter().any(|param| {
            matches!(
                param.kind,
                ParamKind::Keyword | ParamKind::KeywordOpt | ParamKind::KeywordRest
            )
        });
        let requires_keyword = overload
            .params
            .iter()
            .any(|param| param.kind == ParamKind::Keyword);
        if requires_keyword && arguments.is_empty() {
            return Fit::Excluded;
        }
        // A trailing positional hash may be read as the keywords instead.
        if takes_keywords && !arguments.is_empty() {
            return Fit::Undecided;
        }

        let is_required = |param: &MethodParam| param.kind == ParamKind::Required;
        let required = positional.iter().filter(|param| is_required(param)).count();
        let has_rest = positional.iter().any(|param| param.kind == ParamKind::Rest);
        if arguments.len() < required || (!has_rest && arguments.len() > positional.len()) {
            return Fit::Excluded;
        }

        let leading = positional
            .iter()
            .take_while(|param| is_required(param))
            .count();
        let trailing = if leading == positional.len() {
            0
        } else {
            positional
                .iter()
                .rev()
                .take_while(|param| is_required(param))
                .count()
        };
        let middle = &positional[leading..positional.len() - trailing];
        if middle.iter().any(|param| is_required(param)) {
            return Fit::Undecided;
        }
        let optional: Vec<&MethodParam> = middle
            .iter()
            .filter(|param| param.kind == ParamKind::Optional)
            .copied()
            .collect();
        let rest = middle.iter().find(|param| param.kind == ParamKind::Rest);

        let mut fit = Fit::Applies;
        for (index, argument) in arguments.iter().enumerate() {
            let from_end = arguments.len() - index;
            let param = if index < leading {
                positional.get(index)
            } else if from_end <= trailing {
                positional.get(positional.len() - from_end)
            } else {
                optional.get(index - leading).or(rest)
            };
            let Some(param) = param else {
                return Fit::Undecided;
            };
            fit = fit.and(self.type_fit(&param.r#type, argument, 0));
            if fit == Fit::Excluded {
                return Fit::Excluded;
            }
        }
        fit
    }

    fn type_fit(&self, param: &RbsType, argument: &RubyType, depth: usize) -> Fit {
        if depth > MAX_TYPE_DEPTH {
            return Fit::Undecided;
        }
        if let RubyType::Union(members) = argument {
            return Fit::every_alternative(
                members
                    .iter()
                    .map(|member| self.type_fit(param, member, depth + 1)),
            );
        }
        match param {
            RbsType::Untyped | RbsType::Top | RbsType::TypeVar(_) => Fit::Applies,
            RbsType::Optional(inner) => Fit::any(
                [
                    self.class_fit("NilClass", argument),
                    self.type_fit(inner, argument, depth + 1),
                ]
                .into_iter(),
            ),
            RbsType::Union(members) => Fit::any(
                members
                    .iter()
                    .map(|member| self.type_fit(member, argument, depth + 1)),
            ),
            RbsType::Intersection(members) => members
                .iter()
                .map(|member| self.type_fit(member, argument, depth + 1))
                .fold(Fit::Applies, Fit::and),
            RbsType::Nil => self.class_fit("NilClass", argument),
            RbsType::Bool => Fit::any(
                [
                    self.class_fit("TrueClass", argument),
                    self.class_fit("FalseClass", argument),
                ]
                .into_iter(),
            ),
            RbsType::Class(name) | RbsType::ClassInstance { name, .. } => {
                self.named_fit(name.strip_prefix("::").unwrap_or(name), argument, depth)
            }
            RbsType::Interface(name) => {
                self.interface_fit(name.strip_prefix("::").unwrap_or(name), argument)
            }
            RbsType::Tuple(_)
            | RbsType::Record(_)
            | RbsType::Proc { .. }
            | RbsType::Literal(_)
            | RbsType::SelfType
            | RbsType::Instance
            | RbsType::ClassType
            | RbsType::Void
            | RbsType::Bot => Fit::Undecided,
        }
    }

    /// A name in parameter position: a type parameter, a class, an alias, or
    /// an interface. A module, or a name the signatures do not declare,
    /// decides nothing.
    fn named_fit(&self, name: &str, argument: &RubyType, depth: usize) -> Fit {
        let is_type_param = self
            .class_params
            .iter()
            .chain(self.method_params)
            .any(|param| type_param_name(param) == name);
        if is_type_param {
            return Fit::Applies;
        }
        if self.loader.get_class(name).is_some() {
            return self.class_fit(name, argument);
        }
        if let Some(alias) = self.loader.get_type_alias(name) {
            return self.type_fit(&alias.r#type, argument, depth + 1);
        }
        if self.loader.get_interface(name).is_some() {
            return self.interface_fit(name, argument);
        }
        Fit::Undecided
    }

    /// Whether a value of the argument's class is an instance of `class`.
    ///
    /// The argument's class is a lower bound: the value may belong to a
    /// subclass. With single inheritance it is still excluded once the two
    /// classes are on different branches.
    fn class_fit(&self, class: &str, argument: &RubyType) -> Fit {
        let Some(argument_class) = argument_class_name(argument) else {
            return Fit::Undecided;
        };
        if argument_class == class {
            return Fit::Applies;
        }
        let (Some(argument_ancestors), Some(class_ancestors)) =
            (self.superclasses(&argument_class), self.superclasses(class))
        else {
            return Fit::Undecided;
        };
        if argument_ancestors.iter().any(|ancestor| ancestor == class) {
            Fit::Applies
        } else if class_ancestors
            .iter()
            .any(|ancestor| *ancestor == argument_class)
        {
            Fit::Undecided
        } else {
            Fit::Excluded
        }
    }

    /// Whether the argument's class answers every method of the interface.
    /// A bundled class's own signatures are taken as its whole interface.
    fn interface_fit(&self, interface: &str, argument: &RubyType) -> Fit {
        let Some(interface) = self.loader.get_interface(interface) else {
            return Fit::Undecided;
        };
        let Some(argument_class) = argument_class_name(argument) else {
            return Fit::Undecided;
        };
        if self.loader.get_class(&argument_class).is_none() {
            return Fit::Undecided;
        }
        let answered = rbs_method_name_catalog(&argument_class, false);
        if interface
            .methods
            .iter()
            .all(|method| answered.contains(&method.name))
        {
            Fit::Applies
        } else {
            Fit::Excluded
        }
    }

    /// The bundled superclass chain of `class`, nearest first, or `None`
    /// when the signatures do not declare the whole chain.
    fn superclasses(&self, class: &str) -> Option<Vec<String>> {
        let mut chain = Vec::new();
        let mut current = class.to_string();
        for _ in 0..MAX_TYPE_DEPTH {
            let declared = self.loader.get_class(&current)?;
            let parent = match &declared.superclass {
                Some(superclass) => rbs_type_to_class_name(superclass)?,
                None if current == "BasicObject" => return Some(chain),
                None if current == "Object" => "BasicObject".to_string(),
                None => "Object".to_string(),
            };
            chain.push(parent.clone());
            current = parent;
        }
        None
    }
}

/// The class a proven argument type is an instance of, by its full name.
fn argument_class_name(argument: &RubyType) -> Option<String> {
    match argument {
        RubyType::Class(fqn) => {
            let name = fqn
                .namespace_parts()
                .iter()
                .map(|part| part.to_string())
                .collect::<Vec<_>>()
                .join("::");
            (!name.is_empty()).then_some(name)
        }
        RubyType::Array(_) => Some("Array".to_string()),
        RubyType::Hash(_, _) | RubyType::Shape(_) => Some("Hash".to_string()),
        RubyType::Literal(value) => argument_class_name(&value.widened_type()),
        RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}
