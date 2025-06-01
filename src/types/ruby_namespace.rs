use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RubyConstant(String);

impl RubyConstant {
    /// Creates a validated namespace segment.
    /// Returns `Err` if invalid Ruby class/module name.
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("Namespace segment cannot be empty");
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        // Must start with uppercase (Unicode-aware)
        if !unicode_ident::is_xid_start(first) || !first.is_uppercase() {
            return Err("Namespace must start with uppercase letter");
        }

        // Subsequent characters must be word-like (letters, numbers, _)
        if !chars.all(|c| unicode_ident::is_xid_continue(c)) {
            return Err("Namespace contains invalid characters");
        }

        Ok(Self(name.to_string()))
    }

    /// Splits a "Foo::Bar::Baz" string into validated segments.
    pub fn from_qualified_name(fqn: &str) -> Result<Vec<Self>, &'static str> {
        fqn.split("::")
            .map(|segment| RubyConstant::new(segment.trim()))
            .collect()
    }
}

impl TryFrom<&str> for RubyConstant {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        RubyConstant::new(value)
    }
}

impl Display for RubyConstant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let namespace = RubyConstant::new("Foo");
        assert_eq!(namespace.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_from_qualified_name() {
        let namespaces = RubyConstant::from_qualified_name("Foo::Bar::Baz");
        assert_eq!(namespaces.as_ref().unwrap().len(), 3);
        assert_eq!(namespaces.as_ref().unwrap()[0].to_string(), "Foo");
        assert_eq!(namespaces.as_ref().unwrap()[1].to_string(), "Bar");
        assert_eq!(namespaces.as_ref().unwrap()[2].to_string(), "Baz");
    }

    #[test]
    fn test_try_from() {
        let namespace = RubyConstant::try_from("Foo");
        assert_eq!(namespace.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_display() {
        let namespace = RubyConstant::new("Foo").unwrap();
        assert_eq!(namespace.to_string(), "Foo");
    }

    #[test]
    fn test_try_from_invalid() {
        let namespace = RubyConstant::try_from("foo");
        assert!(namespace.is_err());
    }

    #[test]
    fn test_try_from_empty() {
        let namespace = RubyConstant::try_from("");
        assert!(namespace.is_err());
    }
}
