use crate::fully_qualified_name::FullyQualifiedName;
use std::fmt::{self, Display, Formatter};

/// Represents Ruby types in the type inference system
/// Following Ruby's object model where everything is an object
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    Unknown,
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
        // `FalseClass` sorts before `TrueClass`; construct this closed language
        // union directly instead of paying the general flatten/sort/dedup cost.
        RubyType::Union(vec![Self::false_class(), Self::true_class()])
    }

    /// Construct `inner | NilClass` without guessing when `inner` is Unknown.
    ///
    /// The common concrete case needs only one comparison. Existing unions
    /// still use the general constructor so nested alternatives are flattened
    /// and deduplicated canonically.
    pub fn optional(inner: RubyType) -> Self {
        match inner {
            RubyType::Unknown => RubyType::Unknown,
            RubyType::Union(types) => RubyType::union(
                types
                    .into_iter()
                    .chain(std::iter::once(RubyType::nil_class())),
            ),
            inner => {
                let nil = RubyType::nil_class();
                if inner == nil {
                    inner
                } else if inner < nil {
                    RubyType::Union(vec![inner, nil])
                } else {
                    RubyType::Union(vec![nil, inner])
                }
            }
        }
    }

    pub fn array_of(element_type: RubyType) -> Self {
        RubyType::Array(vec![element_type])
    }

    pub fn hash_of(key_type: RubyType, value_type: RubyType) -> Self {
        RubyType::Hash(vec![key_type], vec![value_type])
    }

    /// Normalize an exhaustive set of alternatives for use inside a
    /// structured type such as `Array` or `Hash`.
    ///
    /// Unknown absorbs the alternatives because retaining the known members
    /// beside it would let consumers silently select a partial answer. Empty
    /// sets also become one explicit Unknown type argument.
    pub fn canonical_union_members(types: impl IntoIterator<Item = RubyType>) -> Vec<RubyType> {
        let mut types = types.into_iter();
        let Some(first) = types.next() else {
            return vec![RubyType::Unknown];
        };
        let Some(second) = types.next() else {
            return match first {
                RubyType::Union(_) => match RubyType::union([first]) {
                    RubyType::Union(types) => types,
                    ty => vec![ty],
                },
                ty => vec![ty],
            };
        };

        match RubyType::union(
            std::iter::once(first)
                .chain(std::iter::once(second))
                .chain(types),
        ) {
            RubyType::Union(types) => types,
            ty => vec![ty],
        }
    }

    /// True when `ruby_type` is exact `Unknown` or contains a union with an
    /// exact `Unknown` member anywhere in the tree.
    ///
    /// `RubyType::union` flattens nested unions and absorbs `Unknown`, so a
    /// union containing `Unknown` is not a stable union and must never be
    /// published as proof. `Array([Unknown])` and `Hash([Unknown], [Unknown])`
    /// are legitimate proven outer containers: exact `Unknown` as a container
    /// element is a valid "unknown element" shape and is not flagged.
    pub fn union_members_contain_unknown(ruby_type: &RubyType) -> bool {
        match ruby_type {
            RubyType::Unknown => true,
            RubyType::Union(members) => members.iter().any(|member| {
                member == &RubyType::Unknown || Self::union_members_contain_unknown(member)
            }),
            RubyType::Array(elements) => elements.iter().any(Self::nested_union_contains_unknown),
            RubyType::Hash(keys, values) => {
                keys.iter().any(Self::nested_union_contains_unknown)
                    || values.iter().any(Self::nested_union_contains_unknown)
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_) => false,
        }
    }

    /// True when any part of `ruby_type` is Unknown.
    ///
    /// This is intentionally stricter than `union_members_contain_unknown`:
    /// diagnostics that compare two types require every nested type argument
    /// to be proven. A known outer container such as `Array<?>` remains useful
    /// inference, but it cannot prove a mismatch with `Array<String>`.
    pub fn contains_unknown(ruby_type: &RubyType) -> bool {
        match ruby_type {
            RubyType::Unknown => true,
            RubyType::Array(elements) | RubyType::Union(elements) => {
                elements.iter().any(Self::contains_unknown)
            }
            RubyType::Hash(keys, values) => {
                keys.iter().any(Self::contains_unknown) || values.iter().any(Self::contains_unknown)
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_) => false,
        }
    }

    fn nested_union_contains_unknown(ruby_type: &RubyType) -> bool {
        match ruby_type {
            RubyType::Unknown => false,
            RubyType::Union(members) => members.iter().any(|member| {
                member == &RubyType::Unknown || Self::nested_union_contains_unknown(member)
            }),
            RubyType::Array(elements) => elements.iter().any(Self::nested_union_contains_unknown),
            RubyType::Hash(keys, values) => {
                keys.iter().any(Self::nested_union_contains_unknown)
                    || values.iter().any(Self::nested_union_contains_unknown)
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_) => false,
        }
    }

    /// Resolve every reachable alternative before constructing its union.
    /// Missing or Unknown evidence fails closed instead of being filtered out.
    pub fn union_from_proven<T>(
        alternatives: impl IntoIterator<Item = T>,
        mut resolve: impl FnMut(T) -> Option<RubyType>,
    ) -> Option<RubyType> {
        let mut resolved = Vec::new();
        for alternative in alternatives {
            let ruby_type = resolve(alternative)?;
            if ruby_type == RubyType::Unknown {
                return None;
            }
            resolved.push(ruby_type);
        }
        if resolved.is_empty() {
            return None;
        }
        Some(RubyType::union(resolved))
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
                // Add other types
                other => {
                    type_vec.push(other);
                }
            }
        }

        // Check if Unknown is present (Strict strictness: Unknown absorbs all types)
        if type_vec.contains(&RubyType::Unknown) {
            return RubyType::Unknown;
        }

        // Remove duplicates
        type_vec.sort();
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
            // Unknown is subtype of nothing except itself
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
            RubyType::optional(self)
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

    /// Create a union of this type with another type
    /// Used for merging types at join points in CFG
    pub fn union_with(&self, other: &RubyType) -> RubyType {
        // Handle special cases
        if self == other {
            return self.clone();
        }

        match (self, other) {
            // Unknown absorbs everything (Strict merging)
            (RubyType::Unknown, _) | (_, RubyType::Unknown) => RubyType::Unknown,

            // Merge unions
            (RubyType::Union(types1), RubyType::Union(types2)) => {
                let mut all_types = types1.clone();
                all_types.extend(types2.clone());
                RubyType::union(all_types)
            }

            // Add to existing union
            (RubyType::Union(types), other) | (other, RubyType::Union(types)) => {
                let mut all_types = types.clone();
                all_types.push(other.clone());
                RubyType::union(all_types)
            }

            // Create new union
            (t1, t2) => RubyType::union([t1.clone(), t2.clone()]),
        }
    }

    /// Subtract a type from this type (for type narrowing)
    /// Returns a new type with the specified type removed
    pub fn subtract(&self, other: &RubyType) -> RubyType {
        match self {
            RubyType::Union(types) => {
                let filtered: Vec<RubyType> =
                    types.iter().filter(|t| *t != other).cloned().collect();
                RubyType::union(filtered)
            }
            t if t == other => RubyType::Unknown,
            t => t.clone(),
        }
    }

    /// Create a class type from a name
    pub fn class(name: &str) -> Self {
        RubyType::Class(FullyQualifiedName::try_from(name).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: RubyType::class received invalid Ruby name `{name}`: \
                 {error}. This is a bug because replacing an invalid type identity with Object \
                 would publish a wrong concrete type. Fix: validate source names at the domain \
                 boundary and construct RubyType only from a valid FullyQualifiedName."
            )
        }))
    }
}

impl Display for RubyType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RubyType::Unknown => write!(f, "?"),
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
    #[should_panic(expected = "INVARIANT VIOLATED: RubyType::class received invalid Ruby name")]
    fn invalid_class_name_does_not_fall_back_to_object() {
        let _ = RubyType::class("not::a::valid::constant");
    }

    #[test]
    fn test_collection_types() {
        let array_type = RubyType::array_of(RubyType::integer());
        assert_eq!(array_type.to_string(), "Array<Integer>");

        let hash_type = RubyType::hash_of(RubyType::string(), RubyType::integer());
        assert_eq!(hash_type.to_string(), "Hash<String, Integer>");
    }

    #[test]
    fn recursive_unknown_detection_distinguishes_inference_from_diagnostic_proof() {
        let unknown_array = RubyType::Array(vec![RubyType::Unknown]);
        let nested_unknown_hash = RubyType::Hash(
            vec![RubyType::symbol()],
            vec![RubyType::Array(vec![RubyType::Unknown])],
        );

        assert!(!RubyType::union_members_contain_unknown(&unknown_array));
        assert!(RubyType::contains_unknown(&unknown_array));
        assert!(RubyType::contains_unknown(&nested_unknown_hash));
        assert!(!RubyType::contains_unknown(&RubyType::Array(vec![
            RubyType::string(),
            RubyType::integer(),
        ])));
    }

    #[test]
    fn specialized_closed_unions_remain_canonical_and_proof_first() {
        assert_eq!(RubyType::boolean().to_string(), "(FalseClass | TrueClass)");
        assert_eq!(
            RubyType::optional(RubyType::array_of(RubyType::string())).to_string(),
            "(NilClass | Array<String>)"
        );
        assert_eq!(
            RubyType::optional(RubyType::nil_class()),
            RubyType::nil_class()
        );
        assert_eq!(RubyType::optional(RubyType::Unknown), RubyType::Unknown);
    }

    #[test]
    fn single_canonical_collection_member_avoids_changing_its_type() {
        assert_eq!(
            RubyType::canonical_union_members([RubyType::string()]),
            vec![RubyType::string()]
        );
        assert_eq!(
            RubyType::canonical_union_members(std::iter::empty()),
            vec![RubyType::Unknown]
        );
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
        assert!(RubyType::integer().is_subtype_of(&RubyType::integer()));
        assert!(!RubyType::integer().is_subtype_of(&RubyType::string()));
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

        let int_unknown = RubyType::integer().union_with(&RubyType::Unknown);
        assert_eq!(int_unknown, RubyType::Unknown);
    }
}
