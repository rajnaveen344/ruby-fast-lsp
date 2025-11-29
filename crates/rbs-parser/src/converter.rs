//! Type conversion utilities for RBS types.
//!
//! This module provides functions to convert RBS types to various formats
//! that can be used by other parts of the LSP.

use crate::types::*;

/// Convert an RbsType to a string representation suitable for display or YARD-style type strings
pub fn rbs_type_to_string(rbs_type: &RbsType) -> String {
    match rbs_type {
        RbsType::Class(name) => name.clone(),
        RbsType::ClassInstance { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let args_str: Vec<String> = args.iter().map(rbs_type_to_string).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
        }
        RbsType::Interface(name) => name.clone(),
        RbsType::TypeVar(name) => name.clone(),
        RbsType::Union(types) => {
            let type_strs: Vec<String> = types.iter().map(rbs_type_to_string).collect();
            type_strs.join(" | ")
        }
        RbsType::Intersection(types) => {
            let type_strs: Vec<String> = types.iter().map(rbs_type_to_string).collect();
            type_strs.join(" & ")
        }
        RbsType::Optional(inner) => {
            format!("{}?", rbs_type_to_string(inner))
        }
        RbsType::Tuple(types) => {
            let type_strs: Vec<String> = types.iter().map(rbs_type_to_string).collect();
            format!("[{}]", type_strs.join(", "))
        }
        RbsType::Record(fields) => {
            let field_strs: Vec<String> = fields
                .iter()
                .map(|(name, t)| format!("{}: {}", name, rbs_type_to_string(t)))
                .collect();
            format!("{{ {} }}", field_strs.join(", "))
        }
        RbsType::Proc(method_type) => {
            let params: Vec<String> = method_type
                .params
                .iter()
                .map(|p| rbs_type_to_string(&p.r#type))
                .collect();
            format!(
                "^({}) -> {}",
                params.join(", "),
                rbs_type_to_string(&method_type.return_type)
            )
        }
        RbsType::Literal(lit) => match lit {
            Literal::String(s) => format!("\"{}\"", s),
            Literal::Integer(n) => n.to_string(),
            Literal::Symbol(s) => format!(":{}", s),
            Literal::True => "true".to_string(),
            Literal::False => "false".to_string(),
        },
        RbsType::SelfType => "self".to_string(),
        RbsType::Instance => "instance".to_string(),
        RbsType::ClassType => "class".to_string(),
        RbsType::Void => "void".to_string(),
        RbsType::Nil => "nil".to_string(),
        RbsType::Bool => "bool".to_string(),
        RbsType::Untyped => "untyped".to_string(),
        RbsType::Top => "top".to_string(),
        RbsType::Bot => "bot".to_string(),
    }
}

/// Convert an RbsType to a YARD-compatible type string
/// (e.g., "Array<String>" becomes "Array<String>", "String?" becomes "String, nil")
pub fn rbs_type_to_yard(rbs_type: &RbsType) -> String {
    match rbs_type {
        RbsType::Optional(inner) => {
            format!("{}, nil", rbs_type_to_yard(inner))
        }
        RbsType::Union(types) => {
            let type_strs: Vec<String> = types.iter().map(rbs_type_to_yard).collect();
            type_strs.join(", ")
        }
        RbsType::Bool => "Boolean".to_string(),
        RbsType::Nil => "nil".to_string(),
        RbsType::Void => "void".to_string(),
        _ => rbs_type_to_string(rbs_type),
    }
}

/// Extract the base class name from an RbsType (useful for method lookup)
pub fn get_base_class_name(rbs_type: &RbsType) -> Option<&str> {
    match rbs_type {
        RbsType::Class(name) => Some(name),
        RbsType::ClassInstance { name, .. } => Some(name),
        _ => None,
    }
}

/// Check if an RbsType is nilable (either nil directly or optional)
pub fn is_nilable(rbs_type: &RbsType) -> bool {
    match rbs_type {
        RbsType::Nil => true,
        RbsType::Optional(_) => true,
        RbsType::Union(types) => types.iter().any(is_nilable),
        _ => false,
    }
}

/// Get the non-nil type from an optional or union type
pub fn unwrap_nilable(rbs_type: &RbsType) -> RbsType {
    match rbs_type {
        RbsType::Optional(inner) => (**inner).clone(),
        RbsType::Union(types) => {
            let non_nil: Vec<RbsType> = types
                .iter()
                .filter(|t| !matches!(t, RbsType::Nil))
                .cloned()
                .collect();
            if non_nil.len() == 1 {
                non_nil.into_iter().next().unwrap()
            } else if non_nil.is_empty() {
                RbsType::Nil
            } else {
                RbsType::Union(non_nil)
            }
        }
        _ => rbs_type.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbs_type_to_string_simple() {
        assert_eq!(
            rbs_type_to_string(&RbsType::Class("String".to_string())),
            "String"
        );
        assert_eq!(rbs_type_to_string(&RbsType::Nil), "nil");
        assert_eq!(rbs_type_to_string(&RbsType::Bool), "bool");
        assert_eq!(rbs_type_to_string(&RbsType::Void), "void");
    }

    #[test]
    fn test_rbs_type_to_string_generic() {
        let array_type = RbsType::ClassInstance {
            name: "Array".to_string(),
            args: vec![RbsType::Class("String".to_string())],
        };
        assert_eq!(rbs_type_to_string(&array_type), "Array<String>");

        let hash_type = RbsType::ClassInstance {
            name: "Hash".to_string(),
            args: vec![
                RbsType::Class("Symbol".to_string()),
                RbsType::Class("Integer".to_string()),
            ],
        };
        assert_eq!(rbs_type_to_string(&hash_type), "Hash<Symbol, Integer>");
    }

    #[test]
    fn test_rbs_type_to_string_union() {
        let union_type = RbsType::Union(vec![
            RbsType::Class("String".to_string()),
            RbsType::Class("Integer".to_string()),
        ]);
        assert_eq!(rbs_type_to_string(&union_type), "String | Integer");
    }

    #[test]
    fn test_rbs_type_to_string_optional() {
        let optional_type = RbsType::Optional(Box::new(RbsType::Class("String".to_string())));
        assert_eq!(rbs_type_to_string(&optional_type), "String?");
    }

    #[test]
    fn test_rbs_type_to_yard() {
        let optional_type = RbsType::Optional(Box::new(RbsType::Class("String".to_string())));
        assert_eq!(rbs_type_to_yard(&optional_type), "String, nil");

        assert_eq!(rbs_type_to_yard(&RbsType::Bool), "Boolean");
    }

    #[test]
    fn test_is_nilable() {
        assert!(is_nilable(&RbsType::Nil));
        assert!(is_nilable(&RbsType::Optional(Box::new(RbsType::Class(
            "String".to_string()
        )))));
        assert!(is_nilable(&RbsType::Union(vec![
            RbsType::Class("String".to_string()),
            RbsType::Nil,
        ])));
        assert!(!is_nilable(&RbsType::Class("String".to_string())));
    }

    #[test]
    fn test_unwrap_nilable() {
        let optional = RbsType::Optional(Box::new(RbsType::Class("String".to_string())));
        assert_eq!(
            unwrap_nilable(&optional),
            RbsType::Class("String".to_string())
        );

        let union = RbsType::Union(vec![RbsType::Class("String".to_string()), RbsType::Nil]);
        assert_eq!(unwrap_nilable(&union), RbsType::Class("String".to_string()));
    }
}
