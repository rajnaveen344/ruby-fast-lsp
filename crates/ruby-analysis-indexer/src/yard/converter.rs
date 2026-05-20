//! YARD Type Converter
//!
//! Converts YARD type annotation strings to RubyType enum values.
//! This enables type inference to use YARD documentation for type checking.
//!
//! Type existence validation belongs in the analysis engine, not in this parser.
use ruby_analysis_core::{FullyQualifiedName, RubyConstant};
use ruby_analysis_inference::RubyType;
use tower_lsp::lsp_types::Range;

/// Result of converting a YARD type, including any unresolved type references
#[derive(Debug, Clone)]
pub struct TypeConversionResult {
    /// The converted RubyType
    pub ruby_type: RubyType,
    /// Types that couldn't be found in the index (type_name, range if available)
    pub unresolved_types: Vec<UnresolvedType>,
}

/// An unresolved type reference in YARD documentation
#[derive(Debug, Clone)]
pub struct UnresolvedType {
    /// The type name that couldn't be resolved
    pub type_name: String,
    /// The range in the source where this type appears (for diagnostics)
    pub range: Option<Range>,
}

impl UnresolvedType {
    pub fn new(type_name: String) -> Self {
        Self {
            type_name,
            range: None,
        }
    }

    pub fn with_range(type_name: String, range: Range) -> Self {
        Self {
            type_name,
            range: Some(range),
        }
    }
}

/// Converts YARD type strings to RubyType
pub struct YardTypeConverter;

impl YardTypeConverter {
    /// Convert a single YARD type string to RubyType (without index validation)
    ///
    /// Examples:
    /// - "String" -> RubyType::Class(String)
    /// - "Integer" -> RubyType::Class(Integer)
    /// - "nil" -> RubyType::Class(NilClass)
    /// - "Array<String>" -> RubyType::Array([String])
    /// - "Hash<Symbol, String>" -> RubyType::Hash([Symbol], [String])
    /// - "Hash{Symbol => String}" -> RubyType::Hash([Symbol], [String])
    pub fn convert(type_str: &str) -> RubyType {
        Self::convert_with_validation(type_str).ruby_type
    }

    /// Convert a single YARD type string to RubyType.
    pub fn convert_with_validation(type_str: &str) -> TypeConversionResult {
        let trimmed = type_str.trim();
        let mut unresolved = Vec::new();

        // Handle nil/void specially
        if trimmed.eq_ignore_ascii_case("nil") {
            return TypeConversionResult {
                ruby_type: RubyType::nil_class(),
                unresolved_types: unresolved,
            };
        }
        if trimmed.eq_ignore_ascii_case("void") {
            return TypeConversionResult {
                ruby_type: RubyType::nil_class(),
                unresolved_types: unresolved,
            };
        }
        if trimmed.eq_ignore_ascii_case("true") {
            return TypeConversionResult {
                ruby_type: RubyType::true_class(),
                unresolved_types: unresolved,
            };
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return TypeConversionResult {
                ruby_type: RubyType::false_class(),
                unresolved_types: unresolved,
            };
        }
        if trimmed.eq_ignore_ascii_case("boolean") || trimmed.eq_ignore_ascii_case("bool") {
            return TypeConversionResult {
                ruby_type: RubyType::boolean(),
                unresolved_types: unresolved,
            };
        }

        // Handle Array<T> or Array<T, U, ...>
        if let Some(inner) = Self::extract_generic(trimmed, "Array") {
            let result = Self::parse_type_list_with_validation(&inner);
            unresolved.extend(result.1);
            let element_types = if result.0.is_empty() {
                vec![RubyType::Unknown]
            } else {
                result.0
            };
            return TypeConversionResult {
                ruby_type: RubyType::Array(element_types),
                unresolved_types: unresolved,
            };
        }

        // Handle Hash<K, V> syntax
        if let Some(inner) = Self::extract_generic(trimmed, "Hash") {
            let parts = Self::split_hash_types(&inner);
            if parts.len() >= 2 {
                let key_result = Self::parse_type_list_with_validation(&parts[0]);
                let value_result = Self::parse_type_list_with_validation(&parts[1]);
                unresolved.extend(key_result.1);
                unresolved.extend(value_result.1);
                return TypeConversionResult {
                    ruby_type: RubyType::Hash(key_result.0, value_result.0),
                    unresolved_types: unresolved,
                };
            }
            return TypeConversionResult {
                ruby_type: RubyType::Hash(vec![RubyType::Unknown], vec![RubyType::Unknown]),
                unresolved_types: unresolved,
            };
        }

        // Handle Hash{K => V} syntax
        if let Some(inner) = Self::extract_hash_brace(trimmed) {
            let parts: Vec<&str> = inner.split("=>").collect();
            if parts.len() == 2 {
                let key_result = Self::parse_type_list_with_validation(parts[0].trim());
                let value_result = Self::parse_type_list_with_validation(parts[1].trim());
                unresolved.extend(key_result.1);
                unresolved.extend(value_result.1);
                return TypeConversionResult {
                    ruby_type: RubyType::Hash(key_result.0, value_result.0),
                    unresolved_types: unresolved,
                };
            }
            return TypeConversionResult {
                ruby_type: RubyType::Hash(vec![RubyType::Unknown], vec![RubyType::Unknown]),
                unresolved_types: unresolved,
            };
        }

        // Handle standard types
        let ruby_type = match trimmed {
            "String" => RubyType::string(),
            "Integer" | "Fixnum" | "Bignum" => RubyType::integer(),
            "Float" => RubyType::float(),
            "Symbol" => RubyType::symbol(),
            "TrueClass" => RubyType::true_class(),
            "FalseClass" => RubyType::false_class(),
            "NilClass" => RubyType::nil_class(),
            "Object" | "BasicObject" => RubyType::Unknown,
            // For any other type, parse the constant path. Resolution happens
            // in the analysis engine.
            _ => {
                if let Some(fqn) = Self::parse_type_name_to_fqn(trimmed) {
                    RubyType::Class(fqn)
                } else {
                    unresolved.push(UnresolvedType::new(trimmed.to_string()));
                    RubyType::Unknown
                }
            }
        };

        TypeConversionResult {
            ruby_type,
            unresolved_types: unresolved,
        }
    }

    /// Convert multiple YARD type strings to a single RubyType (union if multiple)
    ///
    /// Examples:
    /// - ["String"] -> RubyType::Class(String)
    /// - ["String", "nil"] -> RubyType::Union([String, NilClass])
    /// - ["String", "Integer", "nil"] -> RubyType::Union([String, Integer, NilClass])
    pub fn convert_multiple(types: &[String]) -> RubyType {
        Self::convert_multiple_with_validation(types).ruby_type
    }

    /// Convert multiple YARD type strings.
    pub fn convert_multiple_with_validation(types: &[String]) -> TypeConversionResult {
        if types.is_empty() {
            return TypeConversionResult {
                ruby_type: RubyType::Unknown,
                unresolved_types: Vec::new(),
            };
        }

        let mut all_unresolved = Vec::new();

        if types.len() == 1 {
            return Self::convert_with_validation(&types[0]);
        }

        let converted: Vec<RubyType> = types
            .iter()
            .map(|t| {
                let result = Self::convert_with_validation(t);
                all_unresolved.extend(result.unresolved_types);
                result.ruby_type
            })
            .collect();

        // Flatten any nested unions and deduplicate
        let mut flattened = Vec::new();
        for t in converted {
            match t {
                RubyType::Union(inner) => flattened.extend(inner),
                other => flattened.push(other),
            }
        }

        // Remove duplicates while preserving order
        let mut seen = Vec::new();
        for t in flattened {
            if !seen.contains(&t) {
                seen.push(t);
            }
        }

        let ruby_type = if seen.len() == 1 {
            seen.into_iter().next().unwrap()
        } else {
            RubyType::Union(seen)
        };

        TypeConversionResult {
            ruby_type,
            unresolved_types: all_unresolved,
        }
    }

    /// Extract the inner type from a generic like "Array<String>" -> "String"
    fn extract_generic(type_str: &str, generic_name: &str) -> Option<String> {
        let prefix = format!("{}<", generic_name);
        if type_str.starts_with(&prefix) && type_str.ends_with('>') {
            Some(type_str[prefix.len()..type_str.len() - 1].to_string())
        } else {
            None
        }
    }

    /// Extract the inner type from Hash{K => V} syntax
    fn extract_hash_brace(type_str: &str) -> Option<String> {
        if type_str.starts_with("Hash{") && type_str.ends_with('}') {
            Some(type_str[5..type_str.len() - 1].to_string())
        } else {
            None
        }
    }

    /// Split hash types "K, V" into ["K", "V"], handling nested generics
    fn split_hash_types(inner: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut depth = 0;

        for c in inner.chars() {
            match c {
                '<' | '{' => {
                    depth += 1;
                    current.push(c);
                }
                '>' | '}' => {
                    depth -= 1;
                    current.push(c);
                }
                ',' if depth == 0 => {
                    parts.push(current.trim().to_string());
                    current = String::new();
                }
                _ => current.push(c),
            }
        }

        if !current.trim().is_empty() {
            parts.push(current.trim().to_string());
        }

        parts
    }

    /// Parse a type list like "String, Integer" into RubyTypes with validation
    fn parse_type_list_with_validation(types_str: &str) -> (Vec<RubyType>, Vec<UnresolvedType>) {
        let parts = Self::split_hash_types(types_str);
        let mut types = Vec::new();
        let mut unresolved = Vec::new();

        for part in parts {
            let result = Self::convert_with_validation(&part);
            types.push(result.ruby_type);
            unresolved.extend(result.unresolved_types);
        }

        (types, unresolved)
    }

    /// Parse a type name string like "Foo::Bar" into a FullyQualifiedName (public version)
    pub fn parse_type_name_to_fqn_public(type_name: &str) -> Option<FullyQualifiedName> {
        Self::parse_type_name_to_fqn(type_name)
    }

    /// Parse a type name string like "Foo::Bar" into a FullyQualifiedName
    fn parse_type_name_to_fqn(type_name: &str) -> Option<FullyQualifiedName> {
        let parts: Vec<&str> = type_name.split("::").collect();
        let mut namespace = Vec::new();

        for part in parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            match RubyConstant::try_from(trimmed) {
                Ok(constant) => namespace.push(constant),
                Err(_) => return None,
            }
        }

        if namespace.is_empty() {
            None
        } else {
            Some(FullyQualifiedName::Constant(namespace))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_basic_types() {
        assert_eq!(YardTypeConverter::convert("String"), RubyType::string());
        assert_eq!(YardTypeConverter::convert("Integer"), RubyType::integer());
        assert_eq!(YardTypeConverter::convert("Float"), RubyType::float());
        assert_eq!(YardTypeConverter::convert("Symbol"), RubyType::symbol());
        assert_eq!(YardTypeConverter::convert("nil"), RubyType::nil_class());
        assert_eq!(YardTypeConverter::convert("true"), RubyType::true_class());
        assert_eq!(YardTypeConverter::convert("false"), RubyType::false_class());
        assert_eq!(YardTypeConverter::convert("Boolean"), RubyType::boolean());
    }

    #[test]
    fn test_convert_array() {
        let result = YardTypeConverter::convert("Array<String>");
        assert!(matches!(result, RubyType::Array(_)));
        if let RubyType::Array(types) = result {
            assert_eq!(types.len(), 1);
            assert_eq!(types[0], RubyType::string());
        }
    }

    #[test]
    fn test_convert_hash_angle_bracket() {
        let result = YardTypeConverter::convert("Hash<Symbol, String>");
        assert!(matches!(result, RubyType::Hash(_, _)));
        if let RubyType::Hash(keys, values) = result {
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0], RubyType::symbol());
            assert_eq!(values.len(), 1);
            assert_eq!(values[0], RubyType::string());
        }
    }

    #[test]
    fn test_convert_hash_brace() {
        let result = YardTypeConverter::convert("Hash{Symbol => String}");
        assert!(matches!(result, RubyType::Hash(_, _)));
        if let RubyType::Hash(keys, values) = result {
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0], RubyType::symbol());
            assert_eq!(values.len(), 1);
            assert_eq!(values[0], RubyType::string());
        }
    }

    #[test]
    fn test_convert_multiple_single() {
        let types = vec!["String".to_string()];
        assert_eq!(
            YardTypeConverter::convert_multiple(&types),
            RubyType::string()
        );
    }

    #[test]
    fn test_convert_multiple_union() {
        let types = vec!["String".to_string(), "nil".to_string()];
        let result = YardTypeConverter::convert_multiple(&types);
        assert!(matches!(result, RubyType::Union(_)));
        if let RubyType::Union(inner) = result {
            assert_eq!(inner.len(), 2);
            assert!(inner.contains(&RubyType::string()));
            assert!(inner.contains(&RubyType::nil_class()));
        }
    }

    #[test]
    fn test_convert_custom_class() {
        let result = YardTypeConverter::convert("MyClass");
        assert!(matches!(result, RubyType::Class(_)));
        if let RubyType::Class(fqn) = result {
            assert_eq!(fqn.to_string(), "MyClass");
        }
    }

    #[test]
    fn test_convert_empty() {
        let types: Vec<String> = vec![];
        assert_eq!(
            YardTypeConverter::convert_multiple(&types),
            RubyType::Unknown
        );
    }

    #[test]
    fn test_convert_with_validation_custom_type() {
        let result = YardTypeConverter::convert_with_validation("UnknownClass");
        assert!(matches!(result.ruby_type, RubyType::Class(_)));
        assert!(result.unresolved_types.is_empty());
    }

    #[test]
    fn test_builtin_types_always_valid() {
        let result = YardTypeConverter::convert_with_validation("String");
        assert!(result.unresolved_types.is_empty());

        let result = YardTypeConverter::convert_with_validation("Integer");
        assert!(result.unresolved_types.is_empty());

        let result = YardTypeConverter::convert_with_validation("nil");
        assert!(result.unresolved_types.is_empty());
    }
}
