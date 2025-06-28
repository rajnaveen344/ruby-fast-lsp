use std::fmt;

use crate::indexer::entry::MethodKind;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RubyMethod(pub String, pub MethodKind);

impl RubyMethod {
    pub fn new(name: &str, kind: MethodKind) -> Result<Self, &'static str> {
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
        Ok(Self(original_name.to_string(), kind))
    }

    pub fn get_name(&self) -> String {
        self.0.clone()
    }

    pub fn get_kind(&self) -> MethodKind {
        self.1
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
        let method = RubyMethod::new("foo", MethodKind::Instance);
        assert_eq!(method.unwrap().to_string(), "foo");
    }

    #[test]
    fn test_from_string() {
        let method = RubyMethod::new("foo", MethodKind::Instance).unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_from_str() {
        let method_try_from = RubyMethod::new("foo", MethodKind::Instance).unwrap();
        assert_eq!(method_try_from.to_string(), "foo");
    }

    #[test]
    fn test_display() {
        let method = RubyMethod::new("foo", MethodKind::Instance).unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_try_from() {
        let method = RubyMethod::new("foo", MethodKind::Instance).unwrap();
        assert_eq!(method.to_string(), "foo");
    }

    #[test]
    fn test_try_from_invalid() {
        let method = RubyMethod::new("Foo", MethodKind::Instance);
        assert!(method.is_err());
    }

    #[test]
    fn test_try_from_empty() {
        let method = RubyMethod::new("", MethodKind::Instance);
        assert!(method.is_err());
    }

    #[test]
    fn test_method_with_question_mark() {
        let method = RubyMethod::new("empty?", MethodKind::Instance).unwrap();
        assert_eq!(method.to_string(), "empty?");
    }

    #[test]
    fn test_method_with_exclamation_mark() {
        let method = RubyMethod::new("save!", MethodKind::Instance).unwrap();
        assert_eq!(method.to_string(), "save!");
    }

    #[test]
    fn test_method_with_equals() {
        let method = RubyMethod::new("name=", MethodKind::Instance).unwrap();
        assert_eq!(method.to_string(), "name=");
    }

    #[test]
    fn test_invalid_suffix_only() {
        let method = RubyMethod::new("?", MethodKind::Instance);
        assert!(method.is_err());

        let method = RubyMethod::new("!", MethodKind::Instance);
        assert!(method.is_err());

        let method = RubyMethod::new("=", MethodKind::Instance);
        assert!(method.is_err());
    }

    #[test]
    fn test_invalid_with_suffix() {
        let method = RubyMethod::new("Invalid?", MethodKind::Instance);
        assert!(method.is_err());

        let method = RubyMethod::new("Invalid!", MethodKind::Instance);
        assert!(method.is_err());

        let method = RubyMethod::new("Invalid=", MethodKind::Instance);
        assert!(method.is_err());
    }
}
