use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_prism::*;

/// Analyzes Ruby literals and determines their types
pub struct LiteralAnalyzer;

impl LiteralAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a node and return its inferred type if it's a literal
    pub fn analyze_literal(&self, node: &Node) -> Option<RubyType> {
        // Handle ProgramNode and StatementsNode by analyzing their first statement
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_literal(&first_stmt);
            }
            return None;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_literal(&first_stmt);
            }
            return None;
        }

        // String literals
        if node.as_string_node().is_some()
            || node.as_interpolated_string_node().is_some()
            || node.as_x_string_node().is_some()
            || node.as_interpolated_x_string_node().is_some()
        {
            return Some(RubyType::string());
        }

        // Numeric literals
        if node.as_integer_node().is_some() {
            return Some(RubyType::integer());
        }
        if node.as_float_node().is_some() {
            return Some(RubyType::float());
        }
        if node.as_rational_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Rational").unwrap(),
            ));
        }
        if node.as_imaginary_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Complex").unwrap(),
            ));
        }

        // Symbol literals
        if node.as_symbol_node().is_some() || node.as_interpolated_symbol_node().is_some() {
            return Some(RubyType::symbol());
        }

        // Boolean and nil literals
        if node.as_true_node().is_some() {
            return Some(RubyType::true_class());
        }
        if node.as_false_node().is_some() {
            return Some(RubyType::false_class());
        }
        if node.as_nil_node().is_some() {
            return Some(RubyType::nil_class());
        }

        // Regular expression literals
        if node.as_regular_expression_node().is_some()
            || node.as_interpolated_regular_expression_node().is_some()
        {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Regexp").unwrap(),
            ));
        }

        // Array literals - analyze element types
        if let Some(array_node) = node.as_array_node() {
            return Some(self.analyze_array_literal(&array_node));
        }

        // Hash literals - analyze key and value types
        if let Some(hash_node) = node.as_hash_node() {
            return Some(self.analyze_hash_literal(&hash_node));
        }

        // Range literals - simplified to just return Range class
        if node.as_range_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Range").unwrap(),
            ));
        }

        // Proc/lambda literals
        if node.as_lambda_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Proc").unwrap(),
            ));
        }

        // Self keyword
        if node.as_self_node().is_some() {
            return Some(RubyType::Any); // Type depends on context
        }

        // Other nodes are not literals
        None
    }

    /// Analyze an array literal and infer element types
    fn analyze_array_literal(&self, array_node: &ArrayNode) -> RubyType {
        let mut element_types = Vec::new();

        for element in array_node.elements().iter() {
            if let Some(element_type) = self.analyze_literal(&element) {
                element_types.push(element_type);
            } else {
                // If we can't infer the type, use Any
                element_types.push(RubyType::Any);
            }
        }

        // If array is empty, return Array with Unknown element type
        if element_types.is_empty() {
            return RubyType::Array(vec![RubyType::Unknown]);
        }

        // Remove duplicate types
        element_types.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        element_types.dedup();

        // Return Array with deduplicated element types
        RubyType::Array(element_types)
    }

    /// Analyze a hash literal and infer key and value types
    fn analyze_hash_literal(&self, hash_node: &HashNode) -> RubyType {
        let mut key_types = Vec::new();
        let mut value_types = Vec::new();

        for element in hash_node.elements().iter() {
            if let Some(assoc_node) = element.as_assoc_node() {
                // Analyze key
                if let Some(key_type) = self.analyze_literal(&assoc_node.key()) {
                    key_types.push(key_type);
                } else {
                    key_types.push(RubyType::Any);
                }

                // Analyze value
                if let Some(value_type) = self.analyze_literal(&assoc_node.value()) {
                    value_types.push(value_type);
                } else {
                    value_types.push(RubyType::Any);
                }
            } else if let Some(assoc_splat_node) = element.as_assoc_splat_node() {
                // Handle splat operator (**hash)
                if let Some(value) = assoc_splat_node.value() {
                    if let Some(splat_type) = self.analyze_literal(&value) {
                        // If it's a hash type, extract its key/value types
                        match splat_type {
                            RubyType::Hash(keys, values) => {
                                key_types.extend(keys);
                                value_types.extend(values);
                            }
                            _ => {
                                // Unknown hash splat, assume Any types
                                key_types.push(RubyType::Any);
                                value_types.push(RubyType::Any);
                            }
                        }
                    }
                }
            }
        }

        // If hash is empty, return Hash with Unknown key/value types
        if key_types.is_empty() && value_types.is_empty() {
            return RubyType::Hash(vec![RubyType::Unknown], vec![RubyType::Unknown]);
        }

        // Remove duplicate key types
        key_types.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        key_types.dedup();

        // Remove duplicate value types
        value_types.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        value_types.dedup();

        // Return Hash with deduplicated key and value types
        RubyType::Hash(key_types, value_types)
    }

    /// Check if a node represents a literal value
    pub fn is_literal(&self, node: &Node) -> bool {
        // Handle ProgramNode and StatementsNode by checking their first statement
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.is_literal(&first_stmt);
            }
            return false;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.is_literal(&first_stmt);
            }
            return false;
        }

        self.analyze_literal(node).is_some()
    }

    /// Get the literal value as a string if possible
    pub fn get_literal_value(&self, node: &Node) -> Option<String> {
        // Handle ProgramNode and StatementsNode by analyzing their first statement
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.get_literal_value(&first_stmt);
            }
            return None;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.get_literal_value(&first_stmt);
            }
            return None;
        }

        if let Some(string_node) = node.as_string_node() {
            return Some(String::from_utf8_lossy(string_node.unescaped()).to_string());
        }

        if let Some(integer_node) = node.as_integer_node() {
            return Some(format!("{:?}", integer_node.value()));
        }

        if let Some(float_node) = node.as_float_node() {
            return Some(format!("{:?}", float_node.value()));
        }

        if node.as_true_node().is_some() {
            return Some("true".to_string());
        }

        if node.as_false_node().is_some() {
            return Some("false".to_string());
        }

        if node.as_nil_node().is_some() {
            return Some("nil".to_string());
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_with_code<F>(source: &str, test_fn: F)
    where
        F: FnOnce(&LiteralAnalyzer, &Node),
    {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();
        let analyzer = LiteralAnalyzer::new();

        if let Some(statements_node) = ast.as_statements_node() {
            if let Some(first_node) = statements_node.body().iter().next() {
                test_fn(&analyzer, &first_node);
            }
        } else {
            test_fn(&analyzer, &ast);
        }
    }

    #[test]
    fn test_string_literal() {
        test_with_code("\"hello\"", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::string());

            let value = analyzer.get_literal_value(node).unwrap();
            assert_eq!(value, "hello");
        });
    }

    #[test]
    fn test_integer_literal() {
        test_with_code("42", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::integer());
        });
    }

    #[test]
    fn test_float_literal() {
        test_with_code("3.14", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::float());
        });
    }

    #[test]
    fn test_boolean_literals() {
        test_with_code("true", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let true_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(true_type, RubyType::true_class());
        });

        test_with_code("false", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let false_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(false_type, RubyType::false_class());
        });
    }

    #[test]
    fn test_nil_literal() {
        test_with_code("nil", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::nil_class());

            let value = analyzer.get_literal_value(node).unwrap();
            assert_eq!(value, "nil");
        });
    }

    #[test]
    fn test_array_literal() {
        test_with_code("[1, 2, 3]", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(
                ruby_type,
                RubyType::Array(vec![RubyType::Class(
                    FullyQualifiedName::try_from("Integer").unwrap()
                )])
            );
        });
    }

    #[test]
    fn test_hash_literal() {
        test_with_code("{a: 1, b: 2}", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(
                ruby_type,
                RubyType::Hash(
                    vec![RubyType::Class(
                        FullyQualifiedName::try_from("Symbol").unwrap()
                    )],
                    vec![RubyType::Class(
                        FullyQualifiedName::try_from("Integer").unwrap()
                    )]
                )
            );
        });
    }

    #[test]
    fn test_non_literal() {
        test_with_code("variable_name", |analyzer, node| {
            assert!(!analyzer.is_literal(node));
            assert!(analyzer.analyze_literal(node).is_none());
        });
    }
}
