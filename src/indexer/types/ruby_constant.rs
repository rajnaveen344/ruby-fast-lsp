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
        let first = chars.next().unwrap();

        // Start with uppercase (Unicode-aware)
        if !unicode_ident::is_xid_start(first) || !first.is_uppercase() {
            return Err("Constant must start with uppercase letter");
        }

        // Allow word-like characters and _
        if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
            return Err("Invalid constant character");
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
        let constant = RubyConstant::new("Foo");
        assert_eq!(constant.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_from_string() {
        let constant = RubyConstant::from(String::from("Foo"));
        assert_eq!(constant.to_string(), "Foo");
    }

    #[test]
    fn test_from_str() {
        // Test the TryFrom implementation indirectly via String conversion first
        let constant_from_string = RubyConstant::from(String::from("Foo"));
        assert_eq!(constant_from_string.to_string(), "Foo");

        // Test TryFrom directly
        let constant_try_from = RubyConstant::try_from("Foo").unwrap();
        assert_eq!(constant_try_from.to_string(), "Foo");
    }

    #[test]
    fn test_display() {
        let constant = RubyConstant::try_from("Foo").unwrap();
        assert_eq!(constant.to_string(), "Foo");
    }

    #[test]
    fn test_try_from() {
        let constant = RubyConstant::try_from("Foo");
        assert_eq!(constant.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_try_from_invalid() {
        let constant = RubyConstant::try_from("foo");
        assert!(constant.is_err());
    }

    #[test]
    fn test_try_from_empty() {
        let constant = RubyConstant::try_from("");
        assert!(constant.is_err());
    }
}
