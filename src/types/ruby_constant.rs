use std::convert::TryFrom;
use std::fmt;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RubyConstant(String);

impl RubyConstant {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("Constant name cannot be empty");
        }

        let mut chars = name.chars();

        // Allow only uppercase letters, digits, and underscores (Unicode-aware)
        if !chars.all(|c| {
            // Must be a valid identifier character
            (unicode_ident::is_xid_continue(c) &&
             // If it's a letter, it must be uppercase
             (!c.is_alphabetic() || c.is_uppercase())) ||
            // Or an underscore
            c == '_'
        }) {
            return Err("Constant must contain only uppercase letters, digits, or underscores");
        }

        Ok(Self(name.to_string()))
    }
}

impl From<String> for RubyConstant {
    fn from(value: String) -> Self {
        RubyConstant(value)
    }
}

impl TryFrom<&str> for RubyConstant {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        RubyConstant::new(value)
    }
}

impl fmt::Display for RubyConstant {
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
        let constant = RubyConstant::new("FOO");
        assert_eq!(constant.unwrap().to_string(), "FOO");
    }

    #[test]
    fn test_from_string() {
        let constant = RubyConstant::from(String::from("FOO"));
        assert_eq!(constant.to_string(), "FOO");
    }

    #[test]
    fn test_from_str() {
        // Test the TryFrom implementation indirectly via String conversion first
        let constant_from_string = RubyConstant::from(String::from("FOO"));
        assert_eq!(constant_from_string.to_string(), "FOO");

        // Test TryFrom directly
        let constant_try_from = RubyConstant::try_from("FOO").unwrap();
        assert_eq!(constant_try_from.to_string(), "FOO");
    }

    #[test]
    fn test_display() {
        let constant = RubyConstant::try_from("FOO").unwrap();
        assert_eq!(constant.to_string(), "FOO");
    }

    #[test]
    fn test_try_from() {
        let constant = RubyConstant::try_from("FOO");
        assert_eq!(constant.unwrap().to_string(), "FOO");
    }

    #[test]
    fn test_try_from_invalid() {
        // Lowercase name
        let constant = RubyConstant::try_from("foo");
        assert!(constant.is_err());

        // Mixed case name (starts with uppercase but has lowercase)
        let constant = RubyConstant::try_from("Foo");
        assert!(constant.is_err());
    }

    #[test]
    fn test_try_from_empty() {
        let constant = RubyConstant::try_from("");
        assert!(constant.is_err());
    }

    #[test]
    fn test_valid_constants_with_digits_and_underscores() {
        // Constant with digits
        let constant = RubyConstant::try_from("VERSION123");
        assert!(constant.is_ok());
        assert_eq!(constant.unwrap().to_string(), "VERSION123");

        // Constant with underscores
        let constant = RubyConstant::try_from("MAX_VALUE");
        assert!(constant.is_ok());
        assert_eq!(constant.unwrap().to_string(), "MAX_VALUE");

        // Constant with both digits and underscores
        let constant = RubyConstant::try_from("API_V2_MAX");
        assert!(constant.is_ok());
        assert_eq!(constant.unwrap().to_string(), "API_V2_MAX");
    }

    #[test]
    fn test_unicode_constants() {
        // Unicode uppercase letters should be allowed
        let constant = RubyConstant::try_from("CAFÉ");
        assert!(constant.is_ok());
        assert_eq!(constant.unwrap().to_string(), "CAFÉ");

        // Unicode uppercase letters with digits
        let constant = RubyConstant::try_from("RÉSUMÉ123");
        assert!(constant.is_ok());
        assert_eq!(constant.unwrap().to_string(), "RÉSUMÉ123");

        // Unicode uppercase letters with underscores
        let constant = RubyConstant::try_from("ÜBER_MAX");
        assert!(constant.is_ok());
        assert_eq!(constant.unwrap().to_string(), "ÜBER_MAX");

        // Unicode mixed case should be rejected
        let constant = RubyConstant::try_from("Café");
        assert!(constant.is_err());
    }
}
