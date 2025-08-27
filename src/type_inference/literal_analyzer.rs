use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::type_inference::ruby_type::RubyType;
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
        if node.as_string_node().is_some() || node.as_interpolated_string_node().is_some() ||
           node.as_x_string_node().is_some() || node.as_interpolated_x_string_node().is_some() {
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
                FullyQualifiedName::try_from("Rational").unwrap()
            ));
        }
        if node.as_imaginary_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Complex").unwrap()
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
        if node.as_regular_expression_node().is_some() || node.as_interpolated_regular_expression_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Regexp").unwrap()
            ));
        }
        
        // Array literals - simplified to just return Array class
        if node.as_array_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Array").unwrap()
            ));
        }
        
        // Hash literals - simplified to just return Hash class
        if node.as_hash_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Hash").unwrap()
            ));
        }
        
        // Range literals - simplified to just return Range class
        if node.as_range_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Range").unwrap()
            ));
        }
        
        // Proc/lambda literals
        if node.as_lambda_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Proc").unwrap()
            ));
        }
        
        // Self keyword
        if node.as_self_node().is_some() {
            return Some(RubyType::Any); // Type depends on context
        }
        
        // Other nodes are not literals
        None
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
        F: FnOnce(&LiteralAnalyzer, &Node)
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
            assert_eq!(ruby_type, RubyType::Class(FullyQualifiedName::try_from("Array").unwrap()));
        });
    }
    
    #[test]
    fn test_hash_literal() {
        test_with_code("{a: 1, b: 2}", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::Class(FullyQualifiedName::try_from("Hash").unwrap()));
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