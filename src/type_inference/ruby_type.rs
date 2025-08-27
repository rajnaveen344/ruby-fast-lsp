use crate::types::fully_qualified_name::FullyQualifiedName;
use std::fmt::{self, Display, Formatter};

/// Represents Ruby types in the type inference system
/// Following Ruby's object model where everything is an object
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyType {
    // Built-in Ruby classes (everything is an object in Ruby)
    Class(FullyQualifiedName),
    Module(FullyQualifiedName),

    // Class reference - represents a class object that can be used for instantiation
    ClassReference(FullyQualifiedName),

    // Module reference - represents a module object that can be used for inclusion/extension
    ModuleReference(FullyQualifiedName),

    // Parameterized collection types with polymorphic support
    Array(Vec<RubyType>),               // Supports multiple element types
    Hash(Vec<RubyType>, Vec<RubyType>), // Supports multiple key/value types

    // Composite types
    Union(Vec<RubyType>),

    // Special types
    Unknown,
    Any,
}

impl RubyType {
    // Helper constructors for common Ruby classes
    pub fn string() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("String").unwrap())
    }

    pub fn integer() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("Integer").unwrap())
    }

    pub fn float() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("Float").unwrap())
    }

    pub fn nil_class() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("NilClass").unwrap())
    }

    pub fn symbol() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("Symbol").unwrap())
    }

    pub fn true_class() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("TrueClass").unwrap())
    }

    pub fn false_class() -> Self {
        RubyType::Class(FullyQualifiedName::try_from("FalseClass").unwrap())
    }

    pub fn boolean() -> Self {
        RubyType::Union(vec![Self::true_class(), Self::false_class()])
    }

    pub fn array_of(element_type: RubyType) -> Self {
        RubyType::Array(vec![element_type])
    }

    pub fn hash_of(key_type: RubyType, value_type: RubyType) -> Self {
        RubyType::Hash(vec![key_type], vec![value_type])
    }

    /// Create a new union type from a collection of types
    pub fn union(types: impl IntoIterator<Item = RubyType>) -> Self {
        let mut type_vec = Vec::new();

        for ty in types {
            match ty {
                // Flatten nested unions
                RubyType::Union(inner_types) => {
                    type_vec.extend(inner_types);
                }
                // Skip Any type as it subsumes all others
                RubyType::Any => return RubyType::Any,
                // Add other types
                other => {
                    type_vec.push(other);
                }
            }
        }

        // Remove duplicates
        type_vec.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        type_vec.dedup();

        match type_vec.len() {
            0 => RubyType::Unknown,
            1 => type_vec.into_iter().next().unwrap(),
            _ => RubyType::Union(type_vec),
        }
    }

    /// Check if this type is a subtype of another type
    pub fn is_subtype_of(&self, other: &RubyType) -> bool {
        match (self, other) {
            // Any type is only subtype of itself
            (RubyType::Any, RubyType::Any) => true,
            (_, RubyType::Any) => true,
            (RubyType::Any, _) => false,

            // Unknown is subtype of nothing except Any and itself
            (RubyType::Unknown, RubyType::Unknown) => true,
            (RubyType::Unknown, _) => false,

            // Same types are subtypes of each other
            (a, b) if a == b => true,

            // Union type handling
            (RubyType::Union(types), other) => types.iter().all(|t| t.is_subtype_of(other)),
            (this, RubyType::Union(types)) => types.iter().any(|t| this.is_subtype_of(t)),

            // Array covariance - all element types must be subtypes
            (RubyType::Array(elem1), RubyType::Array(elem2)) => elem1
                .iter()
                .all(|e1| elem2.iter().any(|e2| e1.is_subtype_of(e2))),

            // Hash covariance - all key/value types must be subtypes
            (RubyType::Hash(k1, v1), RubyType::Hash(k2, v2)) => {
                k1.iter().all(|k| k2.iter().any(|k2| k.is_subtype_of(k2)))
                    && v1.iter().all(|v| v2.iter().any(|v2| v.is_subtype_of(v2)))
            }

            // Class hierarchy (simplified - in real implementation would check inheritance)
            (RubyType::Class(_), RubyType::Class(_)) => false,

            // No other subtype relationships
            _ => false,
        }
    }

    /// Check if this type is compatible with another type (mutual subtyping)
    pub fn is_compatible_with(&self, other: &RubyType) -> bool {
        self.is_subtype_of(other) || other.is_subtype_of(self)
    }

    /// Get the most specific common supertype of two types
    pub fn common_supertype(&self, other: &RubyType) -> RubyType {
        if self.is_subtype_of(other) {
            other.clone()
        } else if other.is_subtype_of(self) {
            self.clone()
        } else {
            // Create union of both types
            RubyType::union([self.clone(), other.clone()])
        }
    }

    /// Check if this is a primitive type
    pub fn is_primitive(&self) -> bool {
        match self {
            RubyType::Class(fqn) => {
                let name = fqn.to_string();
                matches!(
                    name.as_str(),
                    "NilClass"
                        | "TrueClass"
                        | "FalseClass"
                        | "Integer"
                        | "Float"
                        | "String"
                        | "Symbol"
                )
            }
            _ => false,
        }
    }

    /// Check if this is a collection type
    pub fn is_collection(&self) -> bool {
        matches!(self, RubyType::Array(_) | RubyType::Hash(_, _))
    }

    /// Check if this type is nilable (can be nil)
    pub fn is_nilable(&self) -> bool {
        match self {
            RubyType::Class(fqn) if fqn.to_string() == "NilClass" => true,
            RubyType::Union(types) => types.iter().any(|t| t.is_nilable()),
            _ => false,
        }
    }

    /// Make this type nilable by creating a union with Nil
    pub fn make_nilable(self) -> RubyType {
        if self.is_nilable() {
            self
        } else {
            RubyType::union([self, RubyType::nil_class()])
        }
    }

    /// Remove nil from this type
    pub fn remove_nil(self) -> RubyType {
        match self {
            RubyType::Class(fqn) if fqn.to_string() == "NilClass" => RubyType::Unknown,
            RubyType::Union(mut types) => {
                types.retain(
                    |t| !matches!(t, RubyType::Class(fqn) if fqn.to_string() == "NilClass"),
                );
                RubyType::union(types)
            }
            other => other,
        }
    }
}

impl Display for RubyType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RubyType::Unknown => write!(f, "?"),
            RubyType::Any => write!(f, "Any"),
            RubyType::Class(fqn) => write!(f, "{}", fqn),
            RubyType::Module(fqn) => write!(f, "module {}", fqn),
            RubyType::ClassReference(fqn) => write!(f, "Class<{}>", fqn),
            RubyType::ModuleReference(fqn) => write!(f, "Module<{}>", fqn),
            RubyType::Array(elem_types) => {
                if elem_types.len() == 1 {
                    write!(f, "Array<{}>", elem_types[0])
                } else {
                    let type_strs: Vec<String> = elem_types.iter().map(|t| t.to_string()).collect();
                    write!(f, "Array<{}>", type_strs.join(" | "))
                }
            }
            RubyType::Hash(key_types, value_types) => {
                let key_str = if key_types.len() == 1 {
                    key_types[0].to_string()
                } else {
                    let type_strs: Vec<String> = key_types.iter().map(|t| t.to_string()).collect();
                    format!("({})", type_strs.join(" | "))
                };
                let value_str = if value_types.len() == 1 {
                    value_types[0].to_string()
                } else {
                    let type_strs: Vec<String> =
                        value_types.iter().map(|t| t.to_string()).collect();
                    format!("({})", type_strs.join(" | "))
                };
                write!(f, "Hash<{}, {}>", key_str, value_str)
            }
            RubyType::Union(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "({})", type_strs.join(" | "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_types() {
        assert_eq!(RubyType::integer().to_string(), "Integer");
        assert_eq!(RubyType::string().to_string(), "String");
        assert_eq!(RubyType::nil_class().to_string(), "NilClass");
    }

    #[test]
    fn test_collection_types() {
        let array_type = RubyType::array_of(RubyType::integer());
        assert_eq!(array_type.to_string(), "Array<Integer>");

        let hash_type = RubyType::hash_of(RubyType::string(), RubyType::integer());
        assert_eq!(hash_type.to_string(), "Hash<String, Integer>");
    }

    #[test]
    fn test_union_creation() {
        let union = RubyType::union([RubyType::integer(), RubyType::string()]);
        match union {
            RubyType::Union(types) => {
                assert!(types.contains(&RubyType::integer()));
                assert!(types.contains(&RubyType::string()));
                assert_eq!(types.len(), 2);
            }
            _ => panic!("Expected union type"),
        }
    }

    #[test]
    fn test_union_flattening() {
        let inner_union = RubyType::union([RubyType::integer(), RubyType::string()]);
        let outer_union = RubyType::union([inner_union, RubyType::boolean()]);

        match outer_union {
            RubyType::Union(types) => {
                assert!(types.len() >= 3); // Should contain at least integer, string, and boolean components
            }
            _ => panic!("Expected union type"),
        }
    }

    #[test]
    fn test_subtype_relationships() {
        assert!(RubyType::integer().is_subtype_of(&RubyType::Any));
        assert!(RubyType::integer().is_subtype_of(&RubyType::integer()));
        assert!(!RubyType::integer().is_subtype_of(&RubyType::string()));
        assert!(!RubyType::Any.is_subtype_of(&RubyType::integer()));
    }

    #[test]
    fn test_nilable_operations() {
        assert!(!RubyType::integer().is_nilable());
        assert!(RubyType::nil_class().is_nilable());

        let nilable_int = RubyType::integer().make_nilable();
        assert!(nilable_int.is_nilable());

        let non_nil = nilable_int.remove_nil();
        assert!(!non_nil.is_nilable());
        assert_eq!(non_nil, RubyType::integer());
    }

    #[test]
    fn test_primitive_and_collection_checks() {
        assert!(RubyType::integer().is_primitive());
        assert!(RubyType::string().is_primitive());
        assert!(!RubyType::array_of(RubyType::integer()).is_primitive());

        assert!(RubyType::array_of(RubyType::integer()).is_collection());
        assert!(RubyType::hash_of(RubyType::string(), RubyType::integer()).is_collection());
        assert!(!RubyType::integer().is_collection());
    }

    #[test]
    fn test_common_supertype() {
        let int_str_union = RubyType::integer().common_supertype(&RubyType::string());
        match int_str_union {
            RubyType::Union(types) => {
                assert!(types.contains(&RubyType::integer()));
                assert!(types.contains(&RubyType::string()));
            }
            _ => panic!("Expected union type"),
        }

        let int_any = RubyType::integer().common_supertype(&RubyType::Any);
        assert_eq!(int_any, RubyType::Any);
    }
}
