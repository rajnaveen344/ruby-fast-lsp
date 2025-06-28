use std::fmt::{self, Display, Formatter};

use crate::analyzer_prism::Identifier;

use super::{ruby_method::RubyMethod, ruby_namespace::RubyConstant, ruby_variable::RubyVariable};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum FullyQualifiedName {
    /// Represents a class or module (namespace).
    /// Example: `Foo::Bar` → `Namespace(vec!["Foo", "Bar"])`
    Constant(Vec<RubyConstant>),

    /// Instance method, e.g., `Foo#bar` → `InstanceMethod(vec!["Foo"], RubyMethod::new("bar"))`
    InstanceMethod(Vec<RubyConstant>, RubyMethod),

    /// Class/singleton method, e.g., `Foo.bar` → `ClassMethod(vec!["Foo"], RubyMethod::new("bar"))`
    ClassMethod(Vec<RubyConstant>, RubyMethod),

    /// Local variable, e.g., `a = 1` → `LocalVariable("a")`
    Variable(RubyVariable),
}

impl FullyQualifiedName {
    // Eg. Foo::Bar::Baz
    pub fn namespace(namespace: Vec<RubyConstant>) -> Self {
        FullyQualifiedName::Constant(namespace)
    }

    // Eg. a = Foo.new; a.bar
    pub fn instance_method(namespace: Vec<RubyConstant>, method: RubyMethod) -> Self {
        FullyQualifiedName::InstanceMethod(namespace, method)
    }

    // Eg. Foo.bar
    pub fn class_method(namespace: Vec<RubyConstant>, method: RubyMethod) -> Self {
        FullyQualifiedName::ClassMethod(namespace, method)
    }

    // Common accessor for namespace parts
    pub fn namespace_parts(&self) -> Vec<RubyConstant> {
        match self {
            FullyQualifiedName::Constant(ns) => ns.clone(),
            FullyQualifiedName::InstanceMethod(ns, _) => ns.clone(),
            FullyQualifiedName::ClassMethod(ns, _) => ns.clone(),
            FullyQualifiedName::Variable(_) => vec![],
        }
    }

    pub fn variable(variable: RubyVariable) -> Self {
        FullyQualifiedName::Variable(variable)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            FullyQualifiedName::Constant(ns) => ns.is_empty(),
            FullyQualifiedName::InstanceMethod(ns, _) => ns.is_empty(),
            FullyQualifiedName::ClassMethod(ns, _) => ns.is_empty(),
            FullyQualifiedName::Variable(_) => true, // Variables are not namespaced
        }
    }
}

impl From<Identifier> for FullyQualifiedName {
    fn from(value: Identifier) -> Self {
        match value {
            Identifier::RubyConstant(ns) => FullyQualifiedName::Constant(ns),
            Identifier::RubyMethod(ns, method) => FullyQualifiedName::InstanceMethod(ns, method),
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
            FullyQualifiedName::InstanceMethod(_, method) => write!(f, "{namespace}#{method}"),
            FullyQualifiedName::ClassMethod(_, method) => write!(f, "{namespace}.{method}"),
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
        let fqn = FullyQualifiedName::instance_method(
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
        let fqn = FullyQualifiedName::class_method(
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
