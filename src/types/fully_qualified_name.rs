use std::fmt::{self, Display, Formatter};

use crate::{analyzer_prism::Identifier, indexer::entry::MethodKind, types::scope::LVScopeStack};

use super::{ruby_method::RubyMethod, ruby_namespace::RubyConstant};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum FullyQualifiedName {
    /// Represents a class or module (namespace).
    /// Example: `Foo::Bar` → `Namespace(vec!["Foo", "Bar"])`
    Constant(Vec<RubyConstant>),

    /// Represents a method
    /// Example: `Foo::Bar#baz` → `Method(vec!["Foo", "Bar"], RubyMethod::new("baz"))`
    Method(Vec<RubyConstant>, RubyMethod),

    /// Local variable, e.g., `a = 1` → `LocalVariable("a", scope)`
    LocalVariable(String, LVScopeStack),

    /// Instance variable, e.g., `@name` → `InstanceVariable("@name")`
    InstanceVariable(String),

    /// Class variable, e.g., `@@count` → `ClassVariable("@@count")`
    ClassVariable(String),

    /// Global variable, e.g., `$global` → `GlobalVariable("$global")`
    GlobalVariable(String),
}

impl FullyQualifiedName {
    // Eg. Foo::Bar::Baz
    pub fn namespace(namespace: Vec<RubyConstant>) -> Self {
        FullyQualifiedName::Constant(namespace)
    }

    pub fn method(namespace: Vec<RubyConstant>, method: RubyMethod) -> Self {
        FullyQualifiedName::Method(namespace, method)
    }

    pub fn local_variable(name: String, scope: LVScopeStack) -> Result<Self, &'static str> {
        Self::validate_local_variable(&name)?;
        Ok(Self::LocalVariable(name, scope))
    }

    pub fn instance_variable(name: String) -> Result<Self, &'static str> {
        Self::validate_instance_variable(&name)?;
        Ok(Self::InstanceVariable(name))
    }

    pub fn class_variable(name: String) -> Result<Self, &'static str> {
        Self::validate_class_variable(&name)?;
        Ok(Self::ClassVariable(name))
    }

    pub fn global_variable(name: String) -> Result<Self, &'static str> {
        Self::validate_global_variable(&name)?;
        Ok(Self::GlobalVariable(name))
    }

    fn validate_local_variable(name: &str) -> Result<(), &'static str> {
        if name.is_empty() {
            return Err("Local variable name cannot be empty");
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        // Must start with lowercase letter or underscore
        if !(first.is_lowercase() || first == '_') {
            return Err("Local variable name must start with lowercase letter or underscore");
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
            if c.is_ascii_digit() || "_~*$?!\"'&+`.@/;\\=:<>|".contains(c) {
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

    // Common accessor for namespace parts
    pub fn namespace_parts(&self) -> Vec<RubyConstant> {
        match self {
            FullyQualifiedName::Constant(ns) => ns.clone(),
            FullyQualifiedName::Method(ns, _) => ns.clone(),
            FullyQualifiedName::LocalVariable(_, _) => vec![],
            FullyQualifiedName::InstanceVariable(_) => vec![],
            FullyQualifiedName::ClassVariable(_) => vec![],
            FullyQualifiedName::GlobalVariable(_) => vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            FullyQualifiedName::Constant(ns) => ns.is_empty(),
            FullyQualifiedName::Method(ns, _) => ns.is_empty(),
            FullyQualifiedName::LocalVariable(_, _) => true, // Variables are not namespaced
            FullyQualifiedName::InstanceVariable(_) => true,
            FullyQualifiedName::ClassVariable(_) => true,
            FullyQualifiedName::GlobalVariable(_) => true,
        }
    }

    /// Get the final name component (last part of the fully qualified name)
    pub fn name(&self) -> String {
        match self {
            FullyQualifiedName::Constant(ns) => {
                ns.last().map(|c| c.to_string()).unwrap_or_default()
            }
            FullyQualifiedName::Method(_, method) => method.to_string(),
            FullyQualifiedName::LocalVariable(name, _) => name.clone(),
            FullyQualifiedName::InstanceVariable(name) => name.clone(),
            FullyQualifiedName::ClassVariable(name) => name.clone(),
            FullyQualifiedName::GlobalVariable(name) => name.clone(),
        }
    }

    /// Check if this FQN starts with the given prefix
    pub fn starts_with(&self, prefix: &FullyQualifiedName) -> bool {
        let self_parts = self.namespace_parts();
        let prefix_parts = prefix.namespace_parts();

        if prefix_parts.len() > self_parts.len() {
            return false;
        }

        self_parts
            .iter()
            .zip(prefix_parts.iter())
            .all(|(a, b)| a == b)
    }
}

impl From<Identifier> for FullyQualifiedName {
    fn from(value: Identifier) -> Self {
        match value {
            Identifier::RubyConstant { namespace: _, iden } => FullyQualifiedName::Constant(iden),
            Identifier::RubyMethod {
                namespace,
                receiver_kind: _,
                receiver: _,
                iden,
            } => FullyQualifiedName::Method(namespace, iden),
            Identifier::RubyLocalVariable { name, scope, .. } => {
                FullyQualifiedName::LocalVariable(name, scope)
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                FullyQualifiedName::InstanceVariable(name)
            }
            Identifier::RubyClassVariable { name, .. } => FullyQualifiedName::ClassVariable(name),
            Identifier::RubyGlobalVariable { name, .. } => FullyQualifiedName::GlobalVariable(name),
        }
    }
}

impl From<Vec<RubyConstant>> for FullyQualifiedName {
    fn from(value: Vec<RubyConstant>) -> Self {
        FullyQualifiedName::namespace(value)
    }
}

impl TryFrom<&str> for FullyQualifiedName {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err("Empty string cannot be converted to FullyQualifiedName".to_string());
        }

        let parts: Vec<&str> = value.split("::").collect();
        let mut constants = Vec::new();

        for part in parts {
            match RubyConstant::new(part) {
                Ok(constant) => constants.push(constant),
                Err(e) => return Err(format!("Invalid constant '{}': {}", part, e)),
            }
        }

        Ok(FullyQualifiedName::namespace(constants))
    }
}

impl Display for FullyQualifiedName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let namespace = self
            .namespace_parts()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::");

        match self {
            FullyQualifiedName::Constant(_) => write!(f, "{namespace}"),
            FullyQualifiedName::Method(_, method) => match method.get_kind() {
                MethodKind::Instance => write!(f, "{namespace}#{method}"),
                MethodKind::Class => write!(f, "{namespace}.{method}"),
                MethodKind::Unknown => write!(f, "{namespace}?{method}"),
            },
            FullyQualifiedName::LocalVariable(name, _) => write!(f, "{}", name),
            FullyQualifiedName::InstanceVariable(name) => write!(f, "{}", name),
            FullyQualifiedName::ClassVariable(name) => write!(f, "{}", name),
            FullyQualifiedName::GlobalVariable(name) => write!(f, "{}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::indexer::entry::MethodKind;

    use super::*;

    #[test]
    fn test_fully_qualified_name() {
        let fqn = FullyQualifiedName::namespace(vec![
            RubyConstant::new("Foo").unwrap(),
            RubyConstant::new("Bar").unwrap(),
        ]);
        assert_eq!(fqn.to_string(), "Foo::Bar");
    }

    #[test]
    fn test_instance_method() {
        let fqn = FullyQualifiedName::method(
            vec![
                RubyConstant::new("Foo").unwrap(),
                RubyConstant::new("Bar").unwrap(),
            ],
            RubyMethod::new("baz", MethodKind::Instance).unwrap(),
        );
        assert_eq!(fqn.to_string(), "Foo::Bar#baz");
    }

    #[test]
    fn test_class_method() {
        let fqn = FullyQualifiedName::method(
            vec![
                RubyConstant::new("Foo").unwrap(),
                RubyConstant::new("Bar").unwrap(),
            ],
            RubyMethod::new("baz", MethodKind::Class).unwrap(),
        );
        assert_eq!(fqn.to_string(), "Foo::Bar.baz");
    }

    // invalid
    #[test]
    fn test_invalid_namespace() {
        let fqn = FullyQualifiedName::namespace(vec![]);
        assert_eq!(fqn.to_string(), "");
    }

    #[test]
    fn test_namespace_parts() {
        let fqn = FullyQualifiedName::namespace(vec![
            RubyConstant::new("Foo").unwrap(),
            RubyConstant::new("Bar").unwrap(),
        ]);

        assert_eq!(fqn.to_string(), "Foo::Bar");
    }
}
