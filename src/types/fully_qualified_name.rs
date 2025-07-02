use std::fmt::{self, Display, Formatter};

use crate::{analyzer_prism::Identifier, indexer::entry::MethodKind};

use super::{ruby_method::RubyMethod, ruby_namespace::RubyConstant, ruby_variable::RubyVariable};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum FullyQualifiedName {
    /// Represents a class or module (namespace).
    /// Example: `Foo::Bar` → `Namespace(vec!["Foo", "Bar"])`
    Constant(Vec<RubyConstant>),

    /// Represents a method
    /// Example: `Foo::Bar#baz` → `Method(vec!["Foo", "Bar"], RubyMethod::new("baz"))`
    Method(Vec<RubyConstant>, RubyMethod),

    /// Local variable, e.g., `a = 1` → `LocalVariable("a")`
    Variable(RubyVariable),
}

impl FullyQualifiedName {
    // Eg. Foo::Bar::Baz
    pub fn namespace(namespace: Vec<RubyConstant>) -> Self {
        FullyQualifiedName::Constant(namespace)
    }

    pub fn method(namespace: Vec<RubyConstant>, method: RubyMethod) -> Self {
        FullyQualifiedName::Method(namespace, method)
    }

    pub fn variable(variable: RubyVariable) -> Self {
        FullyQualifiedName::Variable(variable)
    }

    // Common accessor for namespace parts
    pub fn namespace_parts(&self) -> Vec<RubyConstant> {
        match self {
            FullyQualifiedName::Constant(ns) => ns.clone(),
            FullyQualifiedName::Method(ns, _) => ns.clone(),
            FullyQualifiedName::Variable(_) => vec![],
        }
    }
}

impl From<Identifier> for FullyQualifiedName {
    fn from(value: Identifier) -> Self {
        match value {
            Identifier::RubyConstant(ns) => FullyQualifiedName::Constant(ns),
            Identifier::RubyMethod(ns, method) => FullyQualifiedName::Method(ns, method),
            _ => panic!("Unsupported identifier type for conversion to FullyQualifiedName"),
        }
    }
}

impl From<Vec<RubyConstant>> for FullyQualifiedName {
    fn from(value: Vec<RubyConstant>) -> Self {
        FullyQualifiedName::namespace(value)
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
            },
            FullyQualifiedName::Variable(variable) => {
                write!(f, "{}", variable)
            }
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
