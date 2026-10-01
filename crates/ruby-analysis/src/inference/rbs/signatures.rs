//! Embedded RBS method signature and return-type lookups with generic substitution.

use rbs_parser::RbsType;

use super::conversion::{rbs_type_to_ruby_type, rbs_type_to_ruby_type_with_substitutions};
use super::RBS_LOADER;
use crate::core::{MethodParamKind, RubyType};

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
