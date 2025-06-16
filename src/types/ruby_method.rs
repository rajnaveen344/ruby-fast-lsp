use std::convert::TryFrom;
use std::fmt;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RubyMethod(String);

impl RubyMethod {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("Method name cannot be empty");
        }

        let original_name = name;
        let mut name_for_validation = name;

        // Handle valid suffixes: ?, !, =
        if let Some(last_char) = name.chars().last() {
            if last_char == '?' || last_char == '!' || last_char == '=' {
                name_for_validation = &name[..name.len() - 1];
            }
        }

        // If name is empty after removing suffix, it's invalid
        if name_for_validation.is_empty() {
            return Err("Method name cannot be just a suffix");
        }

        let mut chars = name_for_validation.chars();
        let first = chars.next().unwrap();

        // Start with lowercase or _
        if !(first.is_lowercase() || first == '_') {
            return Err("Method name must start with lowercase or _");
        }

        // Remaining chars must be XID_continue or _
        if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
            return Err("Invalid character in method name");
        }

        // Use the original name with the suffix preserved
        Ok(Self(original_name.to_string()))
    }
}

impl TryFrom<&str> for RubyMethod {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        RubyMethod::new(value)
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
    use std::convert::TryFrom;

    #[test]
    fn test_new() {
        let method = RubyMethod::new("foo");
        assert_eq!(method.unwrap().to_string(), "foo");
    }

    #[test]
    fn test_from_string() {
        let method = RubyMethod::try_from("foo").unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_from_str() {
        let method_try_from = RubyMethod::try_from("foo").unwrap();
        assert_eq!(method_try_from.to_string(), "foo");
    }

    #[test]
    fn test_display() {
        let method = RubyMethod::try_from("foo").unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_try_from() {
        let method = RubyMethod::try_from("foo");
        assert_eq!(method.unwrap().to_string(), "foo");
    }

    #[test]
    fn test_try_from_invalid() {
        let method = RubyMethod::try_from("Foo");
        assert!(method.is_err());
    }

    #[test]
    fn test_try_from_empty() {
        let method = RubyMethod::try_from("");
        assert!(method.is_err());
    }

    #[test]
    fn test_method_with_question_mark() {
        let method = RubyMethod::try_from("empty?");
        assert_eq!(method.unwrap().to_string(), "empty?");
    }

    #[test]
    fn test_method_with_exclamation_mark() {
        let method = RubyMethod::try_from("save!");
        assert_eq!(method.unwrap().to_string(), "save!");
    }

    #[test]
    fn test_method_with_equals() {
        let method = RubyMethod::try_from("name=");
        assert_eq!(method.unwrap().to_string(), "name=");
    }

    #[test]
    fn test_invalid_suffix_only() {
        let method = RubyMethod::try_from("?");
        assert!(method.is_err());

        let method = RubyMethod::try_from("!");
        assert!(method.is_err());

        let method = RubyMethod::try_from("=");
        assert!(method.is_err());
    }

    #[test]
    fn test_invalid_with_suffix() {
        let method = RubyMethod::try_from("Invalid?");
        assert!(method.is_err());

        let method = RubyMethod::try_from("Invalid!");
        assert!(method.is_err());

        let method = RubyMethod::try_from("Invalid=");
        assert!(method.is_err());
    }
}
