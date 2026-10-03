//! Conversion from RBS types to inferred Ruby types.

use rbs_parser::RbsType;
use std::collections::HashMap;

use crate::core::{
    FullyQualifiedName, LiteralKey, LiteralValue, RubyConstant, RubyType, ShapeExactness,
    ShapeField, ShapeStability, ShapeType,
};

/// Convert an RbsType to a RubyType, substituting type variables with concrete types
pub(super) fn rbs_type_to_ruby_type_with_substitutions(
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
        RbsType::Tuple(types) => {
            RubyType::Array(RubyType::canonical_union_members(types.iter().map(
                |element| rbs_type_to_ruby_type_with_substitutions(element, substitutions),
            )))
        }
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

/// Extract a class/module name from an RbsType (used for ancestor resolution)
pub(super) fn rbs_type_to_class_name(rbs_type: &RbsType) -> Option<String> {
    match rbs_type {
        RbsType::Class(name) => Some(name.strip_prefix("::").unwrap_or(name).to_string()),
        RbsType::ClassInstance { name, .. } => {
            Some(name.strip_prefix("::").unwrap_or(name).to_string())
        }
        _ => None,
    }
}

pub(super) fn substitute_rbs_edge_argument(
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
