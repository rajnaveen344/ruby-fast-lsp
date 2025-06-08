use std::convert::TryFrom;
use std::fmt;

use crate::types::scope_kind::{LVScopeDepth, LVScopeKind};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RubyVariable(String, RubyVariableType);

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum RubyVariableType {
    Local(LVScopeDepth, LVScopeKind),
    Instance,
    Class,
    Global,
}

impl RubyVariable {
    pub fn new(name: &str, variable_type: RubyVariableType) -> Result<Self, &'static str> {
        // Validate the variable name based on its type
        match variable_type {
            RubyVariableType::Local(_, _) => validate_local_variable(name)?,
            RubyVariableType::Instance => validate_instance_variable(name)?,
            RubyVariableType::Class => validate_class_variable(name)?,
            RubyVariableType::Global => validate_global_variable(name)?,
        };

        // Store the name without prefixes to avoid doubling them in Display
        let clean_name = match variable_type {
            RubyVariableType::Local(_, _) => name.to_string(),
            RubyVariableType::Instance => {
                if name.starts_with('@') {
                    name[1..].to_string()
                } else {
                    name.to_string()
                }
            }
            RubyVariableType::Class => {
                if name.starts_with("@@") {
                    name[2..].to_string()
                } else {
                    name.to_string()
                }
            }
            RubyVariableType::Global => {
                if name.starts_with('$') {
                    name[1..].to_string()
                } else {
                    name.to_string()
                }
            }
        };

        Ok(RubyVariable(clean_name, variable_type))
    }

    pub fn name(&self) -> &String {
        &self.0
    }

    pub fn variable_type(&self) -> &RubyVariableType {
        &self.1
    }
}

// Validation functions for different variable types

fn validate_local_variable(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("Local variable name cannot be empty");
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap();

    // Local variables must start with lowercase or underscore
    if !(first.is_lowercase() || first == '_') {
        return Err("Local variable name must start with lowercase or _");
    }

    // Remaining chars must be valid identifiers
    if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
        return Err("Invalid character in local variable name");
    }

    Ok(())
}

fn validate_instance_variable(name: &str) -> Result<(), &'static str> {
    // Instance variables must start with @
    if !name.starts_with('@') {
        return Err("Instance variable name must start with @");
    }

    // Remove the @ prefix for further validation
    let name_without_prefix = &name[1..];
    if name_without_prefix.is_empty() {
        return Err("Instance variable name cannot be just @");
    }

    let mut chars = name_without_prefix.chars();
    let first = chars.next().unwrap();

    // After @ must be a valid identifier start
    if !(unicode_ident::is_xid_start(first) || first == '_') {
        return Err("Invalid character after @ in instance variable name");
    }

    // Remaining chars must be valid identifiers
    if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
        return Err("Invalid character in instance variable name");
    }

    Ok(())
}

fn validate_class_variable(name: &str) -> Result<(), &'static str> {
    // Class variables must start with @@
    if !name.starts_with("@@") {
        return Err("Class variable name must start with @@");
    }

    // Remove the @@ prefix for further validation
    let name_without_prefix = &name[2..];
    if name_without_prefix.is_empty() {
        return Err("Class variable name cannot be just @@");
    }

    let mut chars = name_without_prefix.chars();
    let first = chars.next().unwrap();

    // After @@ must be a valid identifier start
    if !(unicode_ident::is_xid_start(first) || first == '_') {
        return Err("Invalid character after @@ in class variable name");
    }

    // Remaining chars must be valid identifiers
    if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
        return Err("Invalid character in class variable name");
    }

    Ok(())
}

fn validate_global_variable(name: &str) -> Result<(), &'static str> {
    // Global variables must start with $
    if !name.starts_with('$') {
        return Err("Global variable name must start with $");
    }

    // Remove the $ prefix for further validation
    let name_without_prefix = &name[1..];
    if name_without_prefix.is_empty() {
        return Err("Global variable name cannot be just $");
    }

    // Special global variables like $1, $2, $_, $&, etc. are valid
    if name_without_prefix.len() == 1 {
        let c = name_without_prefix.chars().next().unwrap();
        if c.is_digit(10) || "_~*$?!\"\'&+`.@/;\\=:<>|".contains(c) {
            return Ok(());
        }
    }

    let mut chars = name_without_prefix.chars();
    let first = chars.next().unwrap();

    // After $ must be a valid identifier start or special character
    if !(unicode_ident::is_xid_start(first) || first == '_') {
        return Err("Invalid character after $ in global variable name");
    }

    // Remaining chars must be valid identifiers
    if !chars.all(|c| unicode_ident::is_xid_continue(c) || c == '_') {
        return Err("Invalid character in global variable name");
    }

    Ok(())
}

impl TryFrom<(&str, RubyVariableType)> for RubyVariable {
    type Error = &'static str;

    fn try_from(value: (&str, RubyVariableType)) -> Result<Self, Self::Error> {
        let (name, variable_type) = value;
        RubyVariable::new(name, variable_type)
    }
}

impl fmt::Display for RubyVariable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.variable_type() {
            RubyVariableType::Local(_, _) => write!(f, ">{}", self.0),
            RubyVariableType::Instance => write!(f, "@{}", self.0),
            RubyVariableType::Class => write!(f, "@@{}", self.0),
            RubyVariableType::Global => write!(f, "${}", self.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_variable_valid() {
        let result = RubyVariable::new("foo", RubyVariableType::Local(0, LVScopeKind::Method));
        assert!(result.is_ok());
        let var = result.unwrap();
        assert_eq!(var.name(), "foo");
        assert_eq!(
            *var.variable_type(),
            RubyVariableType::Local(0, LVScopeKind::Method)
        );
    }

    #[test]
    fn test_local_variable_with_different_scopes() {
        // Test method scope
        let result = RubyVariable::new("foo", RubyVariableType::Local(0, LVScopeKind::Method));
        assert!(result.is_ok());

        // Test block scope
        let result = RubyVariable::new("bar", RubyVariableType::Local(1, LVScopeKind::Block));
        assert!(result.is_ok());

        // Test top level scope
        let result = RubyVariable::new("baz", RubyVariableType::Local(0, LVScopeKind::TopLevel));
        assert!(result.is_ok());

        // Test explicit block local scope
        let result = RubyVariable::new(
            "qux",
            RubyVariableType::Local(2, LVScopeKind::ExplicitBlockLocal),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_local_variable_with_underscore() {
        let result = RubyVariable::new("_foo", RubyVariableType::Local(0, LVScopeKind::Method));
        assert!(result.is_ok());
    }

    #[test]
    fn test_local_variable_invalid_uppercase() {
        let result = RubyVariable::new("Foo", RubyVariableType::Local(0, LVScopeKind::Method));
        assert!(result.is_err());
    }

    #[test]
    fn test_local_variable_invalid_empty() {
        let result = RubyVariable::new("", RubyVariableType::Local(0, LVScopeKind::Method));
        assert!(result.is_err());
    }

    #[test]
    fn test_instance_variable_valid() {
        let result = RubyVariable::new("@foo", RubyVariableType::Instance);
        assert!(result.is_ok());
    }

    #[test]
    fn test_instance_variable_invalid_no_at() {
        let result = RubyVariable::new("foo", RubyVariableType::Instance);
        assert!(result.is_err());
    }

    #[test]
    fn test_instance_variable_invalid_empty_after_at() {
        let result = RubyVariable::new("@", RubyVariableType::Instance);
        assert!(result.is_err());
    }

    #[test]
    fn test_class_variable_valid() {
        let result = RubyVariable::new("@@foo", RubyVariableType::Class);
        assert!(result.is_ok());
    }

    #[test]
    fn test_class_variable_invalid_single_at() {
        let result = RubyVariable::new("@foo", RubyVariableType::Class);
        assert!(result.is_err());
    }

    #[test]
    fn test_class_variable_invalid_empty_after_at() {
        let result = RubyVariable::new("@@", RubyVariableType::Class);
        assert!(result.is_err());
    }

    #[test]
    fn test_global_variable_valid() {
        let result = RubyVariable::new("$foo", RubyVariableType::Global);
        assert!(result.is_ok());
    }

    #[test]
    fn test_global_variable_special_valid() {
        let result = RubyVariable::new("$1", RubyVariableType::Global);
        assert!(result.is_ok());

        let result = RubyVariable::new("$_", RubyVariableType::Global);
        assert!(result.is_ok());
    }

    #[test]
    fn test_global_variable_invalid_no_dollar() {
        let result = RubyVariable::new("foo", RubyVariableType::Global);
        assert!(result.is_err());
    }

    #[test]
    fn test_global_variable_invalid_empty_after_dollar() {
        let result = RubyVariable::new("$", RubyVariableType::Global);
        assert!(result.is_err());
    }

    #[test]
    fn test_try_from() {
        let result =
            RubyVariable::try_from(("foo", RubyVariableType::Local(0, LVScopeKind::Method)));
        assert!(result.is_ok());

        let result = RubyVariable::try_from(("@bar", RubyVariableType::Instance));
        assert!(result.is_ok());

        let result =
            RubyVariable::try_from(("Foo", RubyVariableType::Local(0, LVScopeKind::Method)));
        assert!(result.is_err());
    }

    #[test]
    fn test_display() {
        // For local variables, the prefix is added by Display
        let var =
            RubyVariable::new("foo", RubyVariableType::Local(0, LVScopeKind::Method)).unwrap();
        assert_eq!(var.to_string(), ">foo");

        // For instance variables, the @ is stripped in try_new and added back in Display
        let var = RubyVariable::new("@bar", RubyVariableType::Instance).unwrap();
        assert_eq!(var.to_string(), "@bar");

        // For class variables, the @@ is stripped in try_new and added back in Display
        let var = RubyVariable::new("@@baz", RubyVariableType::Class).unwrap();
        assert_eq!(var.to_string(), "@@baz");

        // For global variables, the $ is stripped in try_new and added back in Display
        let var = RubyVariable::new("$qux", RubyVariableType::Global).unwrap();
        assert_eq!(var.to_string(), "$qux");
    }
}
