//! RBS Type Index
//!
//! Provides access to RBS type definitions for built-in Ruby classes.
//! The RBS definitions are embedded in the binary at compile time.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rbs_parser::{Loader, RbsType};

use crate::r#type::ruby::RubyType;
use ruby_analysis_core::FullyQualifiedName;
use ruby_analysis_core::RubyConstant;

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
        RbsType::Union(types) => {
            let ruby_types: Vec<RubyType> = types
                .iter()
                .map(|t| rbs_type_to_ruby_type_with_substitutions(t, substitutions))
                .collect();
            if ruby_types.len() == 1 {
                ruby_types.into_iter().next().unwrap()
            } else {
                RubyType::Union(ruby_types)
            }
        }
        RbsType::Optional(inner) => {
            let inner_type = rbs_type_to_ruby_type_with_substitutions(inner, substitutions);
            RubyType::Union(vec![inner_type, RubyType::nil_class()])
        }
        RbsType::ClassInstance { name, args } => {
            let clean_name = name.strip_prefix("::").unwrap_or(name);
            match clean_name {
                "Array" => {
                    let element_types: Vec<RubyType> = args
                        .iter()
                        .map(|t| rbs_type_to_ruby_type_with_substitutions(t, substitutions))
                        .collect();
                    RubyType::Array(element_types)
                }
                "Hash" => {
                    let key_types: Vec<RubyType> = args
                        .first()
                        .map(|t| vec![rbs_type_to_ruby_type_with_substitutions(t, substitutions)])
                        .unwrap_or_default();
                    let value_types: Vec<RubyType> = args
                        .get(1)
                        .map(|t| vec![rbs_type_to_ruby_type_with_substitutions(t, substitutions)])
                        .unwrap_or_default();
                    RubyType::Hash(key_types, value_types)
                }
                _ => class_name_to_ruby_type(clean_name),
            }
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
            RubyType::Unknown
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
            RubyType::Unknown
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

/// Get all methods for a class from RBS definitions, including inherited methods
/// from the ancestor chain (superclass + included modules).
pub fn get_rbs_class_methods(class_name: &str, include_singleton: bool) -> Vec<RbsMethodInfo> {
    let loader = RBS_LOADER.read();
    let mut methods = Vec::new();
    let mut seen_methods = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();

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
}
