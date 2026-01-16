use std::fmt;
use ustr::Ustr;

/// A Ruby method name (just the name, no kind).
/// In the new model, all methods are conceptually "instance methods" of their namespace.
/// The distinction between instance/class methods is encoded in the namespace they belong to:
/// - `Foo#bar` is an instance method on the instance namespace
/// - `#<Class:Foo>#bar` is an instance method on the singleton namespace (class method)
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct RubyMethod(Ustr);

impl RubyMethod {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("Method name cannot be empty");
        }

        // Check if it's a valid Ruby method name
        if Self::is_valid_ruby_method_name(name) {
            Ok(Self(Ustr::from(name)))
        } else {
            Err("Invalid Ruby method name")
        }
    }

    /// Validates if a given string is a valid Ruby method name.
    /// This includes regular method names, operator methods, and special method names.
    ///
    /// # Arguments
    /// * `name` - The method name to validate
    ///
    /// # Returns
    /// * `true` if the name is a valid Ruby method name, `false` otherwise
    pub fn is_valid_ruby_method_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        // Ruby operator methods (binary operators)
        let binary_operators = [
            "+", "-", "*", "/", "%", "**", "==", "!=", "<", "<=", ">", ">=", "<=>", "===", "=~",
            "!~", "&", "|", "^", "<<", ">>", "[]", "[]=",
        ];

        // Ruby unary operators (with @ suffix)
        let unary_operators = ["+@", "-@", "~@", "!@"];

        // Check if it's a binary operator
        if binary_operators.contains(&name) {
            return true;
        }

        // Check if it's a unary operator
        if unary_operators.contains(&name) {
            return true;
        }

        // Handle regular method names with optional suffixes
        let mut name_for_validation = name;

        // Handle valid suffixes: ?, !, =
        if let Some(last_char) = name.chars().last() {
            if last_char == '?' || last_char == '!' || last_char == '=' {
                name_for_validation = &name[..name.len() - 1];
            }
        }

        // If name is empty after removing suffix, check if it's just a suffix operator
        if name_for_validation.is_empty() {
            // Allow standalone operators like "!", "?", "=" only if they're not in our operator list
            return false;
        }

        let mut chars = name_for_validation.chars();
        let first = chars.next().unwrap();

        // Start with letter (uppercase or lowercase) or underscore
        // Ruby allows method names to start with uppercase letters, though it's unconventional
        if !(first.is_alphabetic() || first == '_') {
            return false;
        }

        // Remaining chars must be XID_continue or _
        chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_')
    }

    pub fn get_name(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for RubyMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let method = RubyMethod::new("foo");
        assert_eq!(method.unwrap().to_string(), "foo");
    }

    #[test]
    fn test_from_string() {
        let method = RubyMethod::new("foo").unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_from_str() {
        let method_try_from = RubyMethod::new("foo").unwrap();
        assert_eq!(method_try_from.to_string(), "foo");
    }

    #[test]
    fn test_display() {
        let method = RubyMethod::new("foo").unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_try_from() {
        let method = RubyMethod::new("foo").unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_uppercase_method_names() {
        // Uppercase method names are technically valid in Ruby, though unconventional
        let method = RubyMethod::new("Foo");
        assert!(method.is_ok());
        assert_eq!(method.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_try_from_empty() {
        let method = RubyMethod::new("");
        assert!(method.is_err());
    }

    #[test]
    fn test_method_with_question_mark() {
        let method = RubyMethod::new("empty?").unwrap();
        assert_eq!(method.to_string(), "empty?");
    }

    #[test]
    fn test_method_with_exclamation_mark() {
        let method = RubyMethod::new("save!").unwrap();
        assert_eq!(method.to_string(), "save!");
    }

    #[test]
    fn test_method_with_equals() {
        let method = RubyMethod::new("name=").unwrap();
        assert_eq!(method.to_string(), "name=");
    }

    #[test]
    fn test_invalid_suffix_only() {
        let method = RubyMethod::new("?");
        assert!(method.is_err());

        let method = RubyMethod::new("!");
        assert!(method.is_err());

        let method = RubyMethod::new("=");
        assert!(method.is_err());
    }

    #[test]
    fn test_invalid_with_suffix() {
        // Method names starting with numbers should be invalid even with suffixes
        let method = RubyMethod::new("123invalid?");
        assert!(method.is_err());

        let method = RubyMethod::new("456invalid!");
        assert!(method.is_err());

        let method = RubyMethod::new("789invalid=");
        assert!(method.is_err());
    }

    #[test]
    fn test_ruby_binary_operators() {
        let operators = [
            "+", "-", "*", "/", "%", "**", "==", "!=", "<", "<=", ">", ">=", "<=>", "===", "=~",
            "!~", "&", "|", "^", "<<", ">>", "[]", "[]=",
        ];

        for op in operators {
            let method = RubyMethod::new(op);
            assert!(method.is_ok(), "Operator '{}' should be valid", op);
            assert_eq!(method.unwrap().to_string(), op);
        }
    }

    #[test]
    fn test_ruby_unary_operators() {
        let operators = ["+@", "-@", "~@", "!@"];

        for op in operators {
            let method = RubyMethod::new(op);
            assert!(method.is_ok(), "Unary operator '{}' should be valid", op);
            assert_eq!(method.unwrap().to_string(), op);
        }
    }

    #[test]
    fn test_special_method_names() {
        // Test some common special method names
        let special_methods = ["call", "to_s", "to_i", "to_a", "to_h", "inspect"];

        for method_name in special_methods {
            let method = RubyMethod::new(method_name);
            assert!(method.is_ok(), "Method '{}' should be valid", method_name);
            assert_eq!(method.unwrap().to_string(), method_name);
        }
    }

    #[test]
    fn test_invalid_standalone_suffixes() {
        // These should be invalid as they're just suffixes
        let invalid = ["?", "!", "="];

        for suffix in invalid {
            let method = RubyMethod::new(suffix);
            assert!(
                method.is_err(),
                "Standalone suffix '{}' should be invalid",
                suffix
            );
        }
    }

    #[test]
    fn test_uppercase_with_suffixes() {
        // Uppercase method names with valid suffixes
        let method = RubyMethod::new("Valid?");
        assert!(method.is_ok());
        assert_eq!(method.unwrap().to_string(), "Valid?");

        let method = RubyMethod::new("Save!");
        assert!(method.is_ok());
        assert_eq!(method.unwrap().to_string(), "Save!");

        let method = RubyMethod::new("Name=");
        assert!(method.is_ok());
        assert_eq!(method.unwrap().to_string(), "Name=");
    }

    #[test]
    fn test_mixed_case_method_names() {
        // Mixed case method names
        let method = RubyMethod::new("CamelCase");
        assert!(method.is_ok());
        assert_eq!(method.unwrap().to_string(), "CamelCase");

        let method = RubyMethod::new("XMLParser");
        assert!(method.is_ok());
        assert_eq!(method.unwrap().to_string(), "XMLParser");
    }

    #[test]
    fn test_invalid_method_names() {
        // Method names starting with numbers should be invalid
        let method = RubyMethod::new("123invalid");
        assert!(method.is_err());

        // Method names with invalid characters should be invalid
        let method = RubyMethod::new("invalid-name");
        assert!(method.is_err());

        let method = RubyMethod::new("invalid.name");
        assert!(method.is_err());
    }
}
