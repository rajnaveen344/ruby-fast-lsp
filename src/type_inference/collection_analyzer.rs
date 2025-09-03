use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_prism::*;

/// Analyzer for inferring types of collection elements (arrays, hashes)
pub struct CollectionAnalyzer {
    literal_analyzer: crate::type_inference::literal_analyzer::LiteralAnalyzer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayTypeInfo {
    pub element_types: Vec<RubyType>,
    pub is_homogeneous: bool,
    pub common_type: Option<RubyType>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HashTypeInfo {
    pub key_types: Vec<RubyType>,
    pub value_types: Vec<RubyType>,
    pub is_homogeneous_keys: bool,
    pub is_homogeneous_values: bool,
    pub common_key_type: Option<RubyType>,
    pub common_value_type: Option<RubyType>,
}

impl CollectionAnalyzer {
    pub fn new() -> Self {
        Self {
            literal_analyzer: crate::type_inference::literal_analyzer::LiteralAnalyzer::new(),
        }
    }

    /// Analyze an array node and infer element types
    pub fn analyze_array(&self, node: &Node) -> Option<ArrayTypeInfo> {
        // Handle ProgramNode and StatementsNode
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_array(&first_stmt);
            }
            return None;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_array(&first_stmt);
            }
            return None;
        }

        if let Some(array_node) = node.as_array_node() {
            let mut element_types = Vec::new();

            // Analyze each element in the array
            let elements = array_node.elements();
            for element in elements.iter() {
                if let Some(element_type) = self.infer_element_type(&element) {
                    element_types.push(element_type);
                } else {
                    // If we can't infer the type, assume it's Object
                    element_types.push(RubyType::Class(
                        FullyQualifiedName::try_from("Object").unwrap(),
                    ));
                }
            }

            let is_homogeneous = self.are_types_homogeneous(&element_types);
            let common_type = if is_homogeneous && !element_types.is_empty() {
                Some(element_types[0].clone())
            } else {
                self.find_common_supertype(&element_types)
            };

            Some(ArrayTypeInfo {
                element_types,
                is_homogeneous,
                common_type,
            })
        } else {
            None
        }
    }

    /// Analyze a hash node and infer key/value types
    pub fn analyze_hash(&self, node: &Node) -> Option<HashTypeInfo> {
        // Handle ProgramNode and StatementsNode
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_hash(&first_stmt);
            }
            return None;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_hash(&first_stmt);
            }
            return None;
        }

        if let Some(hash_node) = node.as_hash_node() {
            let mut key_types = Vec::new();
            let mut value_types = Vec::new();

            // Analyze each key-value pair in the hash
            let elements = hash_node.elements();
            for element in elements.iter() {
                if let Some(assoc_node) = element.as_assoc_node() {
                    // Analyze key
                    if let Some(key_type) = self.infer_element_type(&assoc_node.key()) {
                        key_types.push(key_type);
                    } else {
                        key_types.push(RubyType::Class(
                            FullyQualifiedName::try_from("Object").unwrap(),
                        ));
                    }

                    // Analyze value
                    if let Some(value_type) = self.infer_element_type(&assoc_node.value()) {
                        value_types.push(value_type);
                    } else {
                        value_types.push(RubyType::Class(
                            FullyQualifiedName::try_from("Object").unwrap(),
                        ));
                    }
                }
            }

            let is_homogeneous_keys = self.are_types_homogeneous(&key_types);
            let is_homogeneous_values = self.are_types_homogeneous(&value_types);

            let common_key_type = if is_homogeneous_keys && !key_types.is_empty() {
                Some(key_types[0].clone())
            } else {
                self.find_common_supertype(&key_types)
            };

            let common_value_type = if is_homogeneous_values && !value_types.is_empty() {
                Some(value_types[0].clone())
            } else {
                self.find_common_supertype(&value_types)
            };

            Some(HashTypeInfo {
                key_types,
                value_types,
                is_homogeneous_keys,
                is_homogeneous_values,
                common_key_type,
                common_value_type,
            })
        } else {
            None
        }
    }

    /// Infer the type of a single element (could be literal or expression)
    fn infer_element_type(&self, node: &Node) -> Option<RubyType> {
        // First try to analyze as a literal
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(node) {
            return Some(literal_type);
        }

        // Handle nested arrays
        if node.as_array_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Array").unwrap(),
            ));
        }

        // Handle nested hashes
        if node.as_hash_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Hash").unwrap(),
            ));
        }

        // For now, return None for non-literal expressions
        // In a full implementation, this would involve more complex type inference
        None
    }

    /// Check if all types in a collection are the same
    fn are_types_homogeneous(&self, types: &[RubyType]) -> bool {
        if types.is_empty() {
            return true;
        }

        let first_type = &types[0];
        types.iter().all(|t| t == first_type)
    }

    /// Find a common supertype for a collection of types
    fn find_common_supertype(&self, types: &[RubyType]) -> Option<RubyType> {
        if types.is_empty() {
            return None;
        }

        // Simple heuristic: if all types are numeric, return Numeric
        let all_numeric = types.iter().all(|t| {
            matches!(t, RubyType::Class(name) if
                name.to_string() == "Integer" ||
                name.to_string() == "Float" ||
                name.to_string() == "Numeric"
            )
        });

        if all_numeric {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Numeric").unwrap(),
            ));
        }

        // If all types are strings, return String
        let all_strings = types
            .iter()
            .all(|t| matches!(t, RubyType::Class(name) if name.to_string() == "String"));

        if all_strings {
            return Some(RubyType::string());
        }

        // Default to Object as the common supertype
        Some(RubyType::Class(
            FullyQualifiedName::try_from("Object").unwrap(),
        ))
    }

    /// Get the inferred type for an array with polymorphic elements
    pub fn get_array_type(&self, array_info: &ArrayTypeInfo) -> RubyType {
        if let Some(_common_type) = &array_info.common_type {
            // Array[CommonType]
            RubyType::Class(FullyQualifiedName::try_from("Array").unwrap())
        } else {
            // Array[Object] for mixed types
            RubyType::Class(FullyQualifiedName::try_from("Array").unwrap())
        }
    }

    /// Get the inferred type for a hash with polymorphic keys/values
    pub fn get_hash_type(&self, _hash_info: &HashTypeInfo) -> RubyType {
        // For now, just return Hash - in a full implementation,
        // this could be Hash[KeyType, ValueType]
        RubyType::Class(FullyQualifiedName::try_from("Hash").unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_with_code<F>(source: &str, test_fn: F)
    where
        F: FnOnce(&CollectionAnalyzer, &Node),
    {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();
        let analyzer = CollectionAnalyzer::new();

        if let Some(statements_node) = ast.as_statements_node() {
            if let Some(first_node) = statements_node.body().iter().next() {
                test_fn(&analyzer, &first_node);
            }
        } else {
            test_fn(&analyzer, &ast);
        }
    }

    #[test]
    fn test_homogeneous_integer_array() {
        test_with_code("[1, 2, 3]", |analyzer, node| {
            let array_info = analyzer.analyze_array(node).unwrap();
            assert!(array_info.is_homogeneous);
            assert_eq!(array_info.element_types.len(), 3);
            assert_eq!(array_info.common_type, Some(RubyType::integer()));
        });
    }

    #[test]
    fn test_mixed_type_array() {
        test_with_code("[1, \"hello\", true]", |analyzer, node| {
            let array_info = analyzer.analyze_array(node).unwrap();
            assert!(!array_info.is_homogeneous);
            assert_eq!(array_info.element_types.len(), 3);
            // Should find Object as common supertype
            assert_eq!(
                array_info.common_type,
                Some(RubyType::Class(
                    FullyQualifiedName::try_from("Object").unwrap()
                ))
            );
        });
    }

    #[test]
    fn test_numeric_array() {
        test_with_code("[1, 3.14]", |analyzer, node| {
            let array_info = analyzer.analyze_array(node).unwrap();
            assert!(!array_info.is_homogeneous);
            assert_eq!(array_info.element_types.len(), 2);
            // Should find Numeric as common supertype
            assert_eq!(
                array_info.common_type,
                Some(RubyType::Class(
                    FullyQualifiedName::try_from("Numeric").unwrap()
                ))
            );
        });
    }

    #[test]
    fn test_simple_hash() {
        test_with_code("{a: 1, b: 2}", |analyzer, node| {
            let hash_info = analyzer.analyze_hash(node).unwrap();
            assert!(hash_info.is_homogeneous_keys);
            assert!(hash_info.is_homogeneous_values);
            assert_eq!(hash_info.key_types.len(), 2);
            assert_eq!(hash_info.value_types.len(), 2);
        });
    }

    #[test]
    fn test_mixed_hash() {
        test_with_code("{\"name\" => \"John\", :age => 30}", |analyzer, node| {
            let hash_info = analyzer.analyze_hash(node).unwrap();
            assert!(!hash_info.is_homogeneous_keys);
            assert!(!hash_info.is_homogeneous_values);
            assert_eq!(hash_info.key_types.len(), 2);
            assert_eq!(hash_info.value_types.len(), 2);
        });
    }

    #[test]
    fn test_empty_array() {
        test_with_code("[]", |analyzer, node| {
            let array_info = analyzer.analyze_array(node).unwrap();
            assert!(array_info.is_homogeneous);
            assert_eq!(array_info.element_types.len(), 0);
            assert_eq!(array_info.common_type, None);
        });
    }

    #[test]
    fn test_nested_array() {
        test_with_code("[[1, 2], [3, 4]]", |analyzer, node| {
            let array_info = analyzer.analyze_array(node).unwrap();
            assert!(array_info.is_homogeneous);
            assert_eq!(array_info.element_types.len(), 2);
            // All elements should be Array type with Integer elements
            for element_type in &array_info.element_types {
                assert_eq!(
                    *element_type,
                    RubyType::Array(vec![
                        RubyType::Class(FullyQualifiedName::try_from("Integer").unwrap())
                    ])
                );
            }
        });
    }
}
