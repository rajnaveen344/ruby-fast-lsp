use std::convert::TryFrom;
use std::fmt;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RubyMethod(String);

impl RubyMethod {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("Method name cannot be empty");
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        // Start with lowercase or _
        if !(first.is_lowercase() || first == '_') {
            return Err("Method name must start with lowercase or _");
        }

        // Allow word-like characters and _
        if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
            return Err("Invalid method character");
        }

        Ok(Self(name.to_string()))
    }
}

impl From<String> for RubyMethod {
    fn from(value: String) -> Self {
        RubyMethod::new(&value).unwrap()
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
        let method = RubyMethod::from(String::from("foo"));
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
}
