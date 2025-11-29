//! RBS Type Index
//!
//! Provides access to RBS type definitions for built-in Ruby classes.
//! The RBS definitions are embedded in the binary at compile time.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rbs_parser::{Loader, RbsType};

use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;

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

/// Get the return type of a method from RBS, converted to RubyType
pub fn get_rbs_method_return_type_as_ruby_type(
    class_name: &str,
    method_name: &str,
    is_singleton: bool,
) -> Option<RubyType> {
    let rbs_type = get_rbs_method_return_type(class_name, method_name, is_singleton)?;
    Some(rbs_type_to_ruby_type(&rbs_type))
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
        _ => {
            // Try to create an FQN from the class name
            if let Ok(constant) = RubyConstant::new(clean_name) {
                RubyType::Class(FullyQualifiedName::Constant(vec![constant]))
            } else {
                RubyType::Unknown
            }
        }
    }
}

/// Convert an RbsType to a RubyType
pub fn rbs_type_to_ruby_type(rbs_type: &RbsType) -> RubyType {
    match rbs_type {
        RbsType::Void => RubyType::nil_class(),
        RbsType::Nil => RubyType::nil_class(),
        RbsType::Bool => RubyType::Union(vec![RubyType::true_class(), RubyType::false_class()]),
        RbsType::Top | RbsType::Bot | RbsType::Untyped => RubyType::Any,
        RbsType::SelfType => RubyType::Any, // TODO: Track self type in context
        RbsType::Instance => RubyType::Any, // TODO: Track instance type in context
        RbsType::Class(name) => class_name_to_ruby_type(name),
        RbsType::ClassInstance { name, args } => {
            // Strip leading :: from name for matching
            let clean_name = name.strip_prefix("::").unwrap_or(name);
            // Handle generic types like Array[String]
            match clean_name {
                "Array" => {
                    let element_types: Vec<RubyType> =
                        args.iter().map(rbs_type_to_ruby_type).collect();
                    RubyType::Array(element_types)
                }
                "Hash" => {
                    let key_types: Vec<RubyType> = args
                        .first()
                        .map(|t| vec![rbs_type_to_ruby_type(t)])
                        .unwrap_or_default();
                    let value_types: Vec<RubyType> = args
                        .get(1)
                        .map(|t| vec![rbs_type_to_ruby_type(t)])
                        .unwrap_or_default();
                    RubyType::Hash(key_types, value_types)
                }
                _ => class_name_to_ruby_type(clean_name),
            }
        }
        RbsType::ClassType => {
            // The `class` type - represents a class object
            RubyType::Any
        }
        RbsType::Union(types) => {
            let ruby_types: Vec<RubyType> = types.iter().map(rbs_type_to_ruby_type).collect();
            if ruby_types.len() == 1 {
                ruby_types.into_iter().next().unwrap()
            } else {
                RubyType::Union(ruby_types)
            }
        }
        RbsType::Intersection(types) => {
            // For intersections, we just take the first type for now
            types
                .first()
                .map(rbs_type_to_ruby_type)
                .unwrap_or(RubyType::Unknown)
        }
        RbsType::Optional(inner) => {
            let inner_type = rbs_type_to_ruby_type(inner);
            RubyType::Union(vec![inner_type, RubyType::nil_class()])
        }
        RbsType::Tuple(types) => {
            // Represent tuple as Array for now
            let element_types: Vec<RubyType> = types.iter().map(rbs_type_to_ruby_type).collect();
            if element_types.is_empty() {
                RubyType::Array(vec![])
            } else if element_types.iter().all(|t| *t == element_types[0]) {
                // Homogeneous tuple
                RubyType::Array(vec![element_types.into_iter().next().unwrap()])
            } else {
                // Heterogeneous tuple - use union of types
                RubyType::Array(vec![RubyType::Union(element_types)])
            }
        }
        RbsType::Record(_) => {
            // Record types become Hash
            RubyType::Hash(vec![], vec![])
        }
        RbsType::Proc { .. } => {
            // Proc types - just use Proc class for now
            if let Ok(constant) = RubyConstant::new("Proc") {
                RubyType::Class(FullyQualifiedName::Constant(vec![constant]))
            } else {
                RubyType::Unknown
            }
        }
        RbsType::Literal(_) => {
            // Literal types - we can't represent these precisely yet
            RubyType::Unknown
        }
        RbsType::Interface(name) => {
            // Interface types
            if let Ok(constant) = RubyConstant::new(name) {
                RubyType::Class(FullyQualifiedName::Constant(vec![constant]))
            } else {
                RubyType::Unknown
            }
        }
        RbsType::TypeVar(_) => {
            // Type variables like T - can't resolve without context
            RubyType::Any
        }
    }
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

/// Get all methods for a class from RBS definitions
pub fn get_rbs_class_methods(class_name: &str, include_singleton: bool) -> Vec<RbsMethodInfo> {
    let loader = RBS_LOADER.read();
    let mut methods = Vec::new();

    if let Some(class) = loader.get_class(class_name) {
        for method in &class.methods {
            let is_singleton = method.kind == rbs_parser::MethodKind::Singleton;

            // Skip singleton methods if not requested
            if is_singleton && !include_singleton {
                continue;
            }

            // Get parameter names from first overload
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

    // Also check modules (for mixed-in methods)
    if let Some(module) = loader.get_module(class_name) {
        for method in &module.methods {
            let is_singleton = method.kind == rbs_parser::MethodKind::Singleton;

            if is_singleton && !include_singleton {
                continue;
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

    methods
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
    fn test_nonexistent_method() {
        let return_type = get_rbs_method_return_type("String", "nonexistent_method_xyz", false);
        assert!(return_type.is_none(), "Should not find nonexistent method");
    }
}
